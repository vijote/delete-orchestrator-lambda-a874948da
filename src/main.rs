use aws_config::BehaviorVersion;
use aws_sdk_cloudfront::Client as CloudFrontClient;
use aws_sdk_cloudformation::Client as CloudFormationClient;
use aws_sdk_ecr::Client as EcrClient;
use aws_sdk_s3::Client as S3Client;
use lambda_http::{tracing, run, service_fn, Request, Error};

mod event_handler;
use event_handler::function_handler;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let cloudfront_client = CloudFrontClient::new(&config);
    let ecr_client = EcrClient::new(&config);
    let s3_client = S3Client::new(&config);
    let cf_client = CloudFormationClient::new(&config);
    let s3_client_ref = &s3_client;
    let cf_client_ref = &cf_client;
    let client_ref = &cloudfront_client;
    let ecr_client_ref = &ecr_client;

    // Inicializamos el runtime de HTTP
    run(service_fn(move |event: Request| async move {
        function_handler(client_ref, ecr_client_ref, s3_client_ref, cf_client_ref, event).await
    })).await?;

    Ok(())
}