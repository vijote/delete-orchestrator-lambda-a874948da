use std::io::Error;

use aws_sdk_cloudfront::Client as CloudFrontClient;
use lambda_http::{Body, Request, Response};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Payload {
    distribution_id: String,
    policy_id: String,
}

#[derive(Serialize)]
struct ApiResponse {
    message: String,
}

pub async fn function_handler(client: &CloudFrontClient, event: Request) -> Result<Response<Body>, Error> {
    // 1. Parsear el JSON del body de la petición HTTP
    let body_bytes = event.body();
    let payload: Payload = match serde_json::from_slice(body_bytes) {
        Ok(p) => p,
        Err(_) => {
            return Ok(Response::builder()
                .status(400)
                .body(Body::from("Error: JSON inválido o faltan parámetros (distribution_id, policy_id)"))
                .unwrap())
        }
    };

    let dist_id = payload.distribution_id;
    let policy_id = payload.policy_id;

    // 2. Ejecutar la lógica de CloudFront (Igual que antes)
    let dist_config_output = match client.get_distribution_config().id(&dist_id).send().await {
        Ok(out) => out,
        Err(e) => return build_error_response("Error al obtener config de CloudFront", e),
    };

    let dist_etag = dist_config_output.e_tag().unwrap_or("");
    let mut dist_config = dist_config_output.distribution_config().unwrap().clone();

    dist_config.continuous_deployment_policy_id = Some("".to_string());

    if let Err(e) = client.update_distribution().id(&dist_id).distribution_config(dist_config).if_match(dist_etag).send().await {
        return build_error_response("Error al actualizar la distribución", e);
    }

    let policy_output = match client.get_continuous_deployment_policy().id(&policy_id).send().await {
        Ok(out) => out,
        Err(e) => return build_error_response("Error al obtener la política", e),
    };

    let policy_etag = policy_output.e_tag().unwrap_or("");

    if let Err(e) = client.delete_continuous_deployment_policy().id(&policy_id).if_match(policy_etag).send().await {
        return build_error_response("Error al eliminar la política", e);
    }

    // 3. Responder con un HTTP 200 en caso de éxito
    let api_resp = ApiResponse {
        message: format!("Política {} eliminada y distribución {} actualizada correctamente.", policy_id, dist_id),
    };
    
    let resp_body = serde_json::to_string(&api_resp).unwrap();

    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(resp_body))
        .unwrap())
}

// Función auxiliar para formatear errores HTTP 500
fn build_error_response<E: std::fmt::Debug>(ctx: &str, error: E) -> Result<Response<Body>, Error> {
    let err_msg = format!("{{\"error\": \"{}\", \"details\": \"{:?}\"}}", ctx, error);
    Ok(Response::builder()
        .status(500)
        .header("content-type", "application/json")
        .body(Body::from(err_msg))
        .unwrap())
}