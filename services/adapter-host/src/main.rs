#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("NonProxy 适配器宿主启动失败：{}", error.code());
        std::process::exit(1);
    }
}

async fn run() -> Result<(), nonproxy_adapter_host::AdapterHostError> {
    let config = nonproxy_adapter_host::AdapterHostConfig::from_process()?;
    nonproxy_adapter_host::run(config).await
}
