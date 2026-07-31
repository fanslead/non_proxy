mod config;
mod error;
mod server;
mod tls;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("NonProxy 出口探针服务启动失败：{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), error::ProbeServerError> {
    let config = config::ProbeServerConfig::from_process()?;
    let signer = config.load_signer()?;
    let tls = tls::load_server_config(config.certificate_path(), config.tls_key_path())?;
    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    server::serve(listener, tls, signer, config.maximum_connections(), async {
        let _result = tokio::signal::ctrl_c().await;
    })
    .await
}
