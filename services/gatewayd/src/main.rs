use nonproxy_gatewayd::{GatewayConfig, run};

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
