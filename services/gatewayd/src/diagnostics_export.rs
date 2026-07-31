use std::{path::Path, path::PathBuf};

use nonproxy_proto::{
    common::v1::{ErrorDetail, TimeRange},
    control::v1::{DiagnosticRedactionLevel, ExportDiagnosticsRequest, ExportDiagnosticsResponse},
};
use sha2::{Digest, Sha256};
use tonic::Status;

use crate::{
    Gateway, GatewayError,
    clock::{timestamp_from_unix_ms, unix_time_ms},
    diagnostics_document, diagnostics_file,
    diagnostics_redaction::hex,
};

const DEFAULT_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
const MAXIMUM_WINDOW_MS: u64 = DEFAULT_WINDOW_MS;
const MAXIMUM_DOCUMENT_BYTES: usize = 1024 * 1024;
const INCLUDED_SECTIONS: [&str; 6] = [
    "runtime",
    "configuration_summary",
    "component_states",
    "network_and_route_summary",
    "recent_errors",
    "connection_samples",
];

#[derive(Clone, Copy)]
pub(crate) struct DiagnosticsWindow {
    pub start_unix_ms: u64,
    pub end_unix_ms: u64,
}

impl DiagnosticsWindow {
    #[must_use]
    pub const fn contains(self, value: u64) -> bool {
        value >= self.start_unix_ms && value <= self.end_unix_ms
    }
}

pub(crate) struct ExportedDiagnostics {
    diagnostic_id: String,
    local_path: PathBuf,
    size_bytes: usize,
    sha256: [u8; 32],
    redaction_level: DiagnosticRedactionLevel,
    window: DiagnosticsWindow,
    connection_sample_count: usize,
    error_count: usize,
}

#[derive(Debug)]
pub(crate) struct DiagnosticsExportError {
    kind: DiagnosticsExportErrorKind,
    source: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiagnosticsExportErrorKind {
    InvalidRequest,
    State,
    Random,
    Serialization,
    File,
    UnsafePath,
    Collision,
}

pub(crate) async fn export(
    gateway: &Gateway,
    directory: &Path,
    request: ExportDiagnosticsRequest,
) -> Result<ExportedDiagnostics, DiagnosticsExportError> {
    let now = unix_time_ms().map_err(DiagnosticsExportError::state)?;
    let redaction_level = parse_redaction(request.redaction_level)?;
    let window = parse_window(request.time_range.as_ref(), now)?;
    let diagnostic_id = diagnostic_id()?;
    let redaction_salt = random_bytes()?;
    let built = diagnostics_document::build(
        gateway,
        diagnostic_id.clone(),
        now,
        window,
        redaction_level,
        redaction_salt,
    )
    .await
    .map_err(DiagnosticsExportError::state)?;
    let mut content = serde_json::to_vec_pretty(&built.document)
        .map_err(DiagnosticsExportError::serialization)?;
    content.push(b'\n');
    if content.len() > MAXIMUM_DOCUMENT_BYTES {
        return Err(DiagnosticsExportError::new(
            DiagnosticsExportErrorKind::Serialization,
            "诊断文档超出 1 MiB 上限",
        ));
    }
    let sha256: [u8; 32] = Sha256::digest(&content).into();
    let size_bytes = content.len();
    let directory = directory.to_path_buf();
    let file_id = diagnostic_id.clone();
    let local_path = tokio::task::spawn_blocking(move || {
        diagnostics_file::write_private(&directory, &file_id, &content)
    })
    .await
    .map_err(|error| {
        DiagnosticsExportError::new(DiagnosticsExportErrorKind::File, error.to_string())
    })??;

    Ok(ExportedDiagnostics {
        diagnostic_id,
        local_path,
        size_bytes,
        sha256,
        redaction_level,
        window,
        connection_sample_count: built.connection_sample_count,
        error_count: built.error_count,
    })
}

#[must_use]
pub(crate) fn unavailable_response() -> ExportDiagnosticsResponse {
    ExportDiagnosticsResponse {
        diagnostic_id: String::new(),
        local_path: String::new(),
        size_bytes: 0,
        sha256: Vec::new(),
        error: Some(ErrorDetail {
            code: "NP_FEATURE_NOT_AVAILABLE".to_owned(),
            message: "诊断包导出尚未在当前构建中启用。".to_owned(),
            retryable: false,
            metadata: Default::default(),
        }),
        applied_redaction_level: DiagnosticRedactionLevel::Unspecified as i32,
        effective_time_range: None,
        included_sections: Vec::new(),
        connection_sample_count: 0,
        error_count: 0,
    }
}

impl ExportedDiagnostics {
    pub(crate) fn into_response(self) -> Result<ExportDiagnosticsResponse, Status> {
        let local_path = self
            .local_path
            .to_str()
            .ok_or_else(|| Status::internal("诊断包路径不是有效 UTF-8"))?
            .to_owned();
        Ok(ExportDiagnosticsResponse {
            diagnostic_id: self.diagnostic_id,
            local_path,
            size_bytes: u64::try_from(self.size_bytes)
                .map_err(|_| Status::internal("诊断包大小超出协议范围"))?,
            sha256: self.sha256.to_vec(),
            error: None,
            applied_redaction_level: self.redaction_level as i32,
            effective_time_range: Some(TimeRange {
                start: Some(
                    timestamp_from_unix_ms(self.window.start_unix_ms)
                        .map_err(|_| Status::internal("诊断开始时间无效"))?,
                ),
                end: Some(
                    timestamp_from_unix_ms(self.window.end_unix_ms)
                        .map_err(|_| Status::internal("诊断结束时间无效"))?,
                ),
            }),
            included_sections: INCLUDED_SECTIONS.iter().map(ToString::to_string).collect(),
            connection_sample_count: u32::try_from(self.connection_sample_count)
                .map_err(|_| Status::internal("诊断样本数量超出协议范围"))?,
            error_count: u32::try_from(self.error_count)
                .map_err(|_| Status::internal("诊断错误数量超出协议范围"))?,
        })
    }
}

impl DiagnosticsExportError {
    fn new(kind: DiagnosticsExportErrorKind, source: impl Into<String>) -> Self {
        Self {
            kind,
            source: source.into(),
        }
    }

    pub(crate) fn file(source: std::io::Error) -> Self {
        Self::new(DiagnosticsExportErrorKind::File, source.to_string())
    }

    pub(crate) fn unsafe_path() -> Self {
        Self::new(
            DiagnosticsExportErrorKind::UnsafePath,
            "诊断目录或文件路径类型不安全",
        )
    }

    pub(crate) fn collision() -> Self {
        Self::new(
            DiagnosticsExportErrorKind::Collision,
            "随机诊断文件标识发生冲突",
        )
    }

    fn state(source: GatewayError) -> Self {
        Self::new(DiagnosticsExportErrorKind::State, source.to_string())
    }

    fn serialization(source: serde_json::Error) -> Self {
        Self::new(
            DiagnosticsExportErrorKind::Serialization,
            source.to_string(),
        )
    }

    #[must_use]
    pub(crate) fn is_invalid_request(&self) -> bool {
        self.kind == DiagnosticsExportErrorKind::InvalidRequest
    }

    #[must_use]
    pub(crate) const fn user_message(&self) -> &'static str {
        match self.kind {
            DiagnosticsExportErrorKind::InvalidRequest => "诊断范围或脱敏级别无效。",
            DiagnosticsExportErrorKind::State => "无法读取本地诊断状态，请稍后重试。",
            DiagnosticsExportErrorKind::Random => "无法生成安全的诊断包标识。",
            DiagnosticsExportErrorKind::Serialization => "无法生成有界诊断文档。",
            DiagnosticsExportErrorKind::File
            | DiagnosticsExportErrorKind::UnsafePath
            | DiagnosticsExportErrorKind::Collision => {
                "无法安全写入本地诊断文件，请检查磁盘空间和目录权限。"
            }
        }
    }

    #[must_use]
    fn code(&self) -> &'static str {
        match self.kind {
            DiagnosticsExportErrorKind::InvalidRequest => "NP_DIAGNOSTICS_REQUEST_INVALID",
            DiagnosticsExportErrorKind::UnsafePath => "NP_DIAGNOSTICS_PATH_UNSAFE",
            _ => "NP_DIAGNOSTICS_EXPORT_FAILED",
        }
    }

    #[must_use]
    pub(crate) fn into_response(self) -> ExportDiagnosticsResponse {
        let code = self.code();
        let message = self.user_message();
        let retryable = matches!(
            self.kind,
            DiagnosticsExportErrorKind::State | DiagnosticsExportErrorKind::File
        );
        let _internal_source = self.source;
        ExportDiagnosticsResponse {
            error: Some(ErrorDetail {
                code: code.to_owned(),
                message: message.to_owned(),
                retryable,
                metadata: Default::default(),
            }),
            ..unavailable_response()
        }
    }
}

fn parse_redaction(value: i32) -> Result<DiagnosticRedactionLevel, DiagnosticsExportError> {
    match DiagnosticRedactionLevel::try_from(value) {
        Ok(level @ (DiagnosticRedactionLevel::Standard | DiagnosticRedactionLevel::Strict)) => {
            Ok(level)
        }
        _ => Err(DiagnosticsExportError::new(
            DiagnosticsExportErrorKind::InvalidRequest,
            "脱敏级别必须是 standard 或 strict",
        )),
    }
}

fn parse_window(
    value: Option<&TimeRange>,
    now_unix_ms: u64,
) -> Result<DiagnosticsWindow, DiagnosticsExportError> {
    let (start_unix_ms, end_unix_ms) = match value {
        None => (now_unix_ms.saturating_sub(DEFAULT_WINDOW_MS), now_unix_ms),
        Some(value) => {
            let start = value.start.as_ref().and_then(timestamp_unix_ms);
            let end = value.end.as_ref().and_then(timestamp_unix_ms);
            match (start, end) {
                (Some(start), Some(end)) => (start, end),
                _ => return Err(invalid_window("诊断时间范围缺少合法起止时间")),
            }
        }
    };
    if start_unix_ms >= end_unix_ms
        || end_unix_ms > now_unix_ms
        || end_unix_ms.saturating_sub(start_unix_ms) > MAXIMUM_WINDOW_MS
    {
        return Err(invalid_window("诊断时间范围必须位于最近 24 小时内"));
    }
    Ok(DiagnosticsWindow {
        start_unix_ms,
        end_unix_ms,
    })
}

fn timestamp_unix_ms(value: &prost_types::Timestamp) -> Option<u64> {
    let seconds = u64::try_from(value.seconds).ok()?;
    let nanos = u32::try_from(value.nanos).ok()?;
    if nanos >= 1_000_000_000 {
        return None;
    }
    seconds
        .checked_mul(1_000)?
        .checked_add(u64::from(nanos / 1_000_000))
}

fn invalid_window(source: &'static str) -> DiagnosticsExportError {
    DiagnosticsExportError::new(DiagnosticsExportErrorKind::InvalidRequest, source)
}

fn diagnostic_id() -> Result<String, DiagnosticsExportError> {
    let bytes = random_bytes::<16>()?;
    Ok(format!("diag-{}", hex(&bytes)))
}

fn random_bytes<const LENGTH: usize>() -> Result<[u8; LENGTH], DiagnosticsExportError> {
    let mut bytes = [0; LENGTH];
    getrandom::fill(&mut bytes).map_err(|error| {
        DiagnosticsExportError::new(DiagnosticsExportErrorKind::Random, error.to_string())
    })?;
    Ok(bytes)
}
