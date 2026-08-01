use nonproxy_proto::{
    common::v1::{ErrorDetail, PageRequest},
    control::v1::{
        ListSubscriptionSourcesRequest, ListSubscriptionSourcesResponse,
        RefreshSubscriptionSourceRequest, RefreshSubscriptionSourceResponse,
        SubscriptionMutationResult, SubscriptionSourceSummary, UpsertSubscriptionSourceRequest,
        UpsertSubscriptionSourceResponse,
    },
};
use nonproxy_storage::{
    MAXIMUM_REFRESH_INTERVAL_SECONDS, MINIMUM_REFRESH_INTERVAL_SECONDS, SubscriptionSource,
};
use tonic::Status;
use zeroize::Zeroizing;

use crate::{
    Gateway, GatewayError,
    clock::{timestamp_from_unix_ms, unix_time_ms},
    control_mapping,
    subscription_service::SubscriptionService,
    subscription_service_types::{
        SubscriptionRefreshResult, SubscriptionServiceError, SubscriptionUpsert,
    },
};

pub(crate) async fn list(
    gateway: &Gateway,
    request: ListSubscriptionSourcesRequest,
) -> Result<ListSubscriptionSourcesResponse, Status> {
    let sources = gateway
        .list_subscription_sources()
        .await
        .map_err(super::control_rpc_helpers::internal_status)?;
    let page = request.page.unwrap_or(PageRequest {
        page_size: 0,
        page_token: String::new(),
    });
    let (start, end, page) =
        control_mapping::page_bounds(page.page_size, &page.page_token, sources.len())?;
    let sources = sources[start..end]
        .iter()
        .map(source_summary)
        .collect::<Result<Vec<_>, _>>()
        .map_err(super::control_rpc_helpers::internal_status)?;
    Ok(ListSubscriptionSourcesResponse {
        sources,
        page: Some(page),
    })
}

pub(crate) async fn upsert(
    service: &SubscriptionService,
    gateway: &Gateway,
    request: UpsertSubscriptionSourceRequest,
) -> UpsertSubscriptionSourceResponse {
    let result = match refresh_interval_seconds(request.refresh_interval.as_ref()) {
        Ok(refresh_interval_seconds) => match unix_time_ms() {
            Ok(now) => {
                let source_id = request.source_id.clone();
                match service
                    .upsert_at(
                        SubscriptionUpsert {
                            source_id,
                            display_name: request.display_name,
                            endpoint_url: Zeroizing::new(request.endpoint_url),
                            enabled: request.enabled,
                            refresh_interval_seconds,
                            expected_revision: (request.expected_revision > 0)
                                .then_some(request.expected_revision),
                        },
                        now,
                    )
                    .await
                {
                    Ok(result) => mutation_success(gateway, result).await,
                    Err(error) => mutation_error(&error),
                }
            }
            Err(error) => mutation_error(&SubscriptionServiceError::Gateway(error)),
        },
        Err(error) => mutation_error(&error),
    };
    UpsertSubscriptionSourceResponse {
        result: Some(result),
    }
}

pub(crate) async fn refresh(
    service: &SubscriptionService,
    gateway: &Gateway,
    request: RefreshSubscriptionSourceRequest,
) -> RefreshSubscriptionSourceResponse {
    let result = if request.expected_revision == 0 {
        mutation_error(&SubscriptionServiceError::Gateway(
            GatewayError::InvalidContract("刷新订阅必须提供 expected_revision"),
        ))
    } else {
        match unix_time_ms() {
            Ok(now) => match service
                .refresh_at(request.source_id, request.expected_revision, now)
                .await
            {
                Ok(result) => mutation_success(gateway, result).await,
                Err(error) => mutation_error(&error),
            },
            Err(error) => mutation_error(&SubscriptionServiceError::Gateway(error)),
        }
    };
    RefreshSubscriptionSourceResponse {
        result: Some(result),
    }
}

async fn mutation_success(
    gateway: &Gateway,
    result: SubscriptionRefreshResult,
) -> SubscriptionMutationResult {
    let source = gateway.subscription_source(result.source_id.clone()).await;
    match source {
        Ok(Some(source)) => match source_summary(&source) {
            Ok(source) => SubscriptionMutationResult {
                source: Some(source),
                content_unchanged: result.unchanged,
                warnings: cleanup_warnings(result.cleanup_failures),
                error: None,
            },
            Err(error) => gateway_mutation_error(&error),
        },
        Ok(None) => {
            gateway_mutation_error(&GatewayError::InvalidContract("订阅刷新后未能读取权威状态"))
        }
        Err(error) => gateway_mutation_error(&error),
    }
}

fn mutation_error(error: &SubscriptionServiceError) -> SubscriptionMutationResult {
    let mut metadata = std::collections::HashMap::new();
    if error.cleanup_failures() > 0 {
        metadata.insert(
            "credential_cleanup_failures".to_owned(),
            error.cleanup_failures().to_string(),
        );
    }
    SubscriptionMutationResult {
        source: None,
        content_unchanged: false,
        warnings: cleanup_warnings(error.cleanup_failures()),
        error: Some(ErrorDetail {
            code: error.code().to_owned(),
            message: error.to_string(),
            retryable: error.retryable(),
            metadata,
        }),
    }
}

fn gateway_mutation_error(error: &GatewayError) -> SubscriptionMutationResult {
    SubscriptionMutationResult {
        source: None,
        content_unchanged: false,
        warnings: Vec::new(),
        error: Some(control_mapping::error_detail(error)),
    }
}

fn source_summary(source: &SubscriptionSource) -> Result<SubscriptionSourceSummary, GatewayError> {
    Ok(SubscriptionSourceSummary {
        id: source.id().to_owned(),
        display_name: source.display_name().to_owned(),
        enabled: source.enabled(),
        refresh_interval: Some(prost_types::Duration {
            seconds: i64::from(source.refresh_interval_seconds()),
            nanos: 0,
        }),
        revision: source.revision(),
        content_generation: source.content_generation(),
        consecutive_failures: source.consecutive_failures(),
        next_refresh_at: Some(timestamp_from_unix_ms(source.next_refresh_at_unix_ms())?),
        last_attempted_at: source
            .last_attempted_at_unix_ms()
            .map(timestamp_from_unix_ms)
            .transpose()?,
        last_succeeded_at: source
            .last_succeeded_at_unix_ms()
            .map(timestamp_from_unix_ms)
            .transpose()?,
        last_error_code: source.last_error_code().unwrap_or_default().to_owned(),
        node_count: source.node_count(),
    })
}

fn refresh_interval_seconds(
    value: Option<&prost_types::Duration>,
) -> Result<u32, SubscriptionServiceError> {
    let value = value.ok_or_else(|| {
        SubscriptionServiceError::Gateway(GatewayError::InvalidContract("缺少订阅刷新间隔"))
    })?;
    if value.nanos != 0 || value.seconds < 0 {
        return Err(SubscriptionServiceError::Gateway(
            GatewayError::InvalidContract("订阅刷新间隔无效"),
        ));
    }
    let seconds = u32::try_from(value.seconds).map_err(|_| {
        SubscriptionServiceError::Gateway(GatewayError::InvalidContract("订阅刷新间隔超出范围"))
    })?;
    if !(MINIMUM_REFRESH_INTERVAL_SECONDS..=MAXIMUM_REFRESH_INTERVAL_SECONDS).contains(&seconds) {
        return Err(SubscriptionServiceError::Gateway(
            GatewayError::InvalidContract("订阅刷新间隔超出范围"),
        ));
    }
    Ok(seconds)
}

fn cleanup_warnings(cleanup_failures: usize) -> Vec<String> {
    if cleanup_failures == 0 {
        Vec::new()
    } else {
        vec!["部分旧订阅凭据未能清理，可在系统凭据库中手动删除未引用项。".to_owned()]
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_storage::{
        CredentialKind, CredentialReference, MINIMUM_REFRESH_INTERVAL_SECONDS, SubscriptionSource,
    };

    use super::{refresh_interval_seconds, source_summary};

    #[test]
    fn summary_never_contains_url_reference_or_credential_label() {
        let credential = CredentialReference::new(
            "subscription:office:url:secret-reference",
            CredentialKind::SubscriptionUrl,
            "private token label",
            1,
        )
        .unwrap_or_else(|error| panic!("测试订阅 URL 凭据创建失败: {error}"));
        let source = SubscriptionSource::new(
            "office",
            "办公室订阅",
            credential,
            MINIMUM_REFRESH_INTERVAL_SECONDS,
            1,
            1_000,
        )
        .unwrap_or_else(|error| panic!("测试订阅源创建失败: {error}"));

        let summary =
            source_summary(&source).unwrap_or_else(|error| panic!("测试订阅摘要创建失败: {error}"));
        let debug = format!("{summary:?}");
        assert!(!debug.contains("secret-reference"));
        assert!(!debug.contains("private token"));
        assert_eq!(summary.id, "office");
    }

    #[test]
    fn refresh_interval_requires_whole_seconds_within_storage_bounds() {
        let valid = prost_types::Duration {
            seconds: i64::from(MINIMUM_REFRESH_INTERVAL_SECONDS),
            nanos: 0,
        };
        let fractional = prost_types::Duration {
            seconds: i64::from(MINIMUM_REFRESH_INTERVAL_SECONDS),
            nanos: 1,
        };

        assert_eq!(
            refresh_interval_seconds(Some(&valid)).ok(),
            Some(MINIMUM_REFRESH_INTERVAL_SECONDS)
        );
        assert!(refresh_interval_seconds(Some(&fractional)).is_err());
        assert!(refresh_interval_seconds(None).is_err());
    }
}
