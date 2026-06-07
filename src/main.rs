use aws_config::BehaviorVersion;
use aws_sdk_cloudfront::Client as CloudFrontClient;
use lambda_http::{tracing, run, service_fn, Request, Error};

mod event_handler;
use event_handler::function_handler;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let cloudfront_client = CloudFrontClient::new(&config);
    let client_ref = &cloudfront_client;

    // Inicializamos el runtime de HTTP
    run(service_fn(move |event: Request| async move {
        function_handler(client_ref, event).await
    })).await?;

    Ok(())
}