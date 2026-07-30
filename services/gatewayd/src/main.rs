#[cfg(not(windows))]
use nonproxy_gatewayd::{GatewayConfig, run};

#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    let config = match GatewayConfig::from_process() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("gatewayd 配置无效 [{}]: {}", error.code(), error);
            std::process::exit(2);
        }
    };
    if let Err(error) = run(config).await {
        eprintln!("gatewayd 启动失败 [{}]: {}", error.code(), error);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn main() {
    if std::env::args_os()
        .skip(1)
        .any(|argument| argument == "--console")
    {
        run_windows_console();
        return;
    }
    if let Err(error) = nonproxy_gatewayd::run_windows_service_dispatcher() {
        eprintln!("gatewayd 无法连接 Windows Service Control Manager: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_windows_console() {
    let config = match nonproxy_gatewayd::GatewayConfig::from_process() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("gatewayd 配置无效 [{}]: {}", error.code(), error);
            std::process::exit(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("gatewayd 运行时创建失败: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = runtime.block_on(nonproxy_gatewayd::run(config)) {
        eprintln!("gatewayd 启动失败 [{}]: {}", error.code(), error);
        std::process::exit(1);
    }
}
