use std::{ffi::OsString, time::Duration};

use thiserror::Error;
use tokio::{
    runtime::Builder,
    sync::{oneshot, watch},
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

use crate::{GatewayConfig, GatewayError, server};

const SERVICE_NAME: &str = "NonProxyGateway";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
const STARTUP_WAIT_HINT: Duration = Duration::from_secs(20);
const SHUTDOWN_WAIT_HINT: Duration = Duration::from_secs(20);
const SERVICE_EXIT_STARTUP_FAILED: u32 = 1;

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_arguments: Vec<OsString>) {
    let _service_result = run_service();
}

fn run_service() -> windows_service::Result<()> {
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let event_handler = move |event| match event {
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Preshutdown => {
            let _send_result = shutdown_sender.send(true);
            ServiceControlHandlerResult::NoError
        }
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
    status_handle.set_service_status(service_status(ServiceState::StartPending, 1, None))?;

    let result = run_worker(status_handle, shutdown_receiver);
    let exit_code = if result.is_ok() {
        None
    } else {
        Some(SERVICE_EXIT_STARTUP_FAILED)
    };
    status_handle.set_service_status(service_status(ServiceState::Stopped, 0, exit_code))?;
    Ok(())
}

fn run_worker(
    status_handle: ServiceStatusHandle,
    shutdown_receiver: watch::Receiver<bool>,
) -> Result<(), ServiceHostError> {
    let config = GatewayConfig::from_process()?;
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ServiceHostError::Runtime)?;
    runtime.block_on(run_async_worker(status_handle, config, shutdown_receiver))
}

async fn run_async_worker(
    status_handle: ServiceStatusHandle,
    config: GatewayConfig,
    shutdown_receiver: watch::Receiver<bool>,
) -> Result<(), ServiceHostError> {
    let (ready_sender, mut ready_receiver) = oneshot::channel();
    let shutdown_handle = status_handle;
    let shutdown = wait_for_service_stop(shutdown_receiver, shutdown_handle);
    let server = server::run_windows_service(config, shutdown, ready_sender);
    tokio::pin!(server);

    tokio::select! {
        result = &mut server => {
            return match result {
                Ok(()) => Err(ServiceHostError::Readiness),
                Err(error) => Err(ServiceHostError::Gateway(error)),
            };
        }
        ready = &mut ready_receiver => {
            ready.map_err(|_| ServiceHostError::Readiness)?;
        }
    }
    status_handle.set_service_status(service_status(ServiceState::Running, 0, None))?;
    server.await.map_err(ServiceHostError::Gateway)
}

async fn wait_for_service_stop(
    mut receiver: watch::Receiver<bool>,
    status_handle: ServiceStatusHandle,
) {
    if !*receiver.borrow() {
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                break;
            }
        }
    }
    let _status_result =
        status_handle.set_service_status(service_status(ServiceState::StopPending, 1, None));
}

fn service_status(
    state: ServiceState,
    checkpoint: u32,
    failure_code: Option<u32>,
) -> ServiceStatus {
    let pending = matches!(
        state,
        ServiceState::StartPending | ServiceState::StopPending
    );
    let controls_accepted = if state == ServiceState::Running {
        ServiceControlAccept::STOP | ServiceControlAccept::PRESHUTDOWN
    } else {
        ServiceControlAccept::empty()
    };
    let wait_hint = match state {
        ServiceState::StartPending => STARTUP_WAIT_HINT,
        ServiceState::StopPending => SHUTDOWN_WAIT_HINT,
        _ => Duration::ZERO,
    };
    ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: state,
        controls_accepted,
        exit_code: failure_code.map_or(ServiceExitCode::NO_ERROR, ServiceExitCode::ServiceSpecific),
        checkpoint: if pending { checkpoint } else { 0 },
        wait_hint,
        process_id: None,
    }
}

#[derive(Debug, Error)]
enum ServiceHostError {
    #[error("Windows Service 状态操作失败: {0}")]
    Windows(#[from] windows_service::Error),
    #[error("Windows Service 运行时创建失败: {0}")]
    Runtime(std::io::Error),
    #[error("Windows Service 网关启动失败: {0}")]
    Gateway(#[from] GatewayError),
    #[error("Windows Service 未收到网关就绪信号")]
    Readiness,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows_service::service::{ServiceControlAccept, ServiceExitCode, ServiceState};

    use super::{SHUTDOWN_WAIT_HINT, STARTUP_WAIT_HINT, service_status};

    #[test]
    fn running_status_accepts_only_stop_and_preshutdown() {
        let status = service_status(ServiceState::Running, 99, None);

        assert_eq!(status.current_state, ServiceState::Running);
        assert_eq!(
            status.controls_accepted,
            ServiceControlAccept::STOP | ServiceControlAccept::PRESHUTDOWN
        );
        assert_eq!(status.exit_code, ServiceExitCode::NO_ERROR);
        assert_eq!(status.checkpoint, 0);
        assert_eq!(status.wait_hint, Duration::ZERO);
    }

    #[test]
    fn pending_statuses_preserve_checkpoint_and_publish_wait_hint() {
        for (state, expected_wait_hint) in [
            (ServiceState::StartPending, STARTUP_WAIT_HINT),
            (ServiceState::StopPending, SHUTDOWN_WAIT_HINT),
        ] {
            let status = service_status(state, 7, None);

            assert_eq!(status.current_state, state);
            assert!(status.controls_accepted.is_empty());
            assert_eq!(status.checkpoint, 7);
            assert_eq!(status.wait_hint, expected_wait_hint);
        }
    }

    #[test]
    fn stopped_failure_uses_service_specific_exit_and_clears_pending_fields() {
        let status = service_status(ServiceState::Stopped, 7, Some(41));

        assert_eq!(status.current_state, ServiceState::Stopped);
        assert!(status.controls_accepted.is_empty());
        assert_eq!(status.exit_code, ServiceExitCode::ServiceSpecific(41));
        assert_eq!(status.checkpoint, 0);
        assert_eq!(status.wait_hint, Duration::ZERO);
    }
}
