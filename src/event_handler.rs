use std::io::Error;

use aws_sdk_cloudfront::Client as CloudFrontClient;
use lambda_http::{Body, Request, Response};
use aws_sdk_ecr::Client as EcrClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::types::{ObjectIdentifier, Delete};
use serde::{Deserialize, Serialize};
use aws_sdk_cloudformation::Client as CfClient;

#[derive(Deserialize)]
struct Payload {
    distribution_id: String,
    policy_id: String,
    bucket_name: String,
    ecr_repo_1: String,
    ecr_repo_2: String,
    stack_name: String,
}

#[derive(Serialize)]
struct ApiResponse {
    message: String,
}

pub async fn function_handler(
    client: &CloudFrontClient,
    ecr_client: &EcrClient,
    s3_client: &S3Client,
    cloudformation_client: &CfClient,
    event: Request
) -> Result<Response<Body>, Error> {
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

    // --- PASO B: Vaciar Bucket de S3 ---
    if let Err(err_msg) = empty_s3_bucket(s3_client, &payload.bucket_name).await {
        return build_error_response("S3 de-provisioning falló", err_msg);
    }

    // --- PASO C: Vaciar los 2 repositorios de ECR ---
    if let Err(err_msg) = empty_ecr_repository(ecr_client, &payload.ecr_repo_1).await {
        return build_error_response("ECR Repo 1 de-provisioning falló", err_msg);
    }
    if let Err(err_msg) = empty_ecr_repository(ecr_client, &payload.ecr_repo_2).await {
        return build_error_response("ECR Repo 2 de-provisioning falló", err_msg);
    }

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

    // --- PASO D: Eliminar el Stack de CloudFormation ---
    if let Err(err_msg) = delete_cf_stack(cloudformation_client, &payload.stack_name).await {
        return build_error_response("CloudFormation de-provisioning falló", err_msg);
    }

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

async fn empty_ecr_repository(client: &EcrClient, repo_name: &str) -> Result<(), String> {
    let image_ids_output = client.list_images().repository_name(repo_name).send().await
        .map_err(|e| format!("Error listando imágenes en ECR ({}): {:?}", repo_name, e))?;

    if let Some(ids) = image_ids_output.image_ids {
        if !ids.is_empty() {
            client.batch_delete_image()
                .repository_name(repo_name)
                .set_image_ids(Some(ids))
                .send()
                .await
                .map_err(|e| format!("Error borrando imágenes en ECR ({}): {:?}", repo_name, e))?;
        }
    }
    Ok(())
}

async fn empty_s3_bucket(client: &S3Client, bucket: &str) -> Result<(), String> {
    // 1. Listar los objetos
    let objects = client.list_objects_v2().bucket(bucket).send().await
        .map_err(|e| format!("Error listando objetos en S3: {:?}", e))?;

    if let Some(contents) = objects.contents {
        if !contents.is_empty() {
            let mut delete_objects: Vec<ObjectIdentifier> = Vec::new();
            for obj in contents {
                if let Some(key) = obj.key {
                    delete_objects.push(ObjectIdentifier::builder().key(key).build().unwrap());
                }
            }

            // 2. Borrar en lote
            let delete_payload = Delete::builder().set_objects(Some(delete_objects)).build().unwrap();
            client.delete_objects().bucket(bucket).delete(delete_payload).send().await
                .map_err(|e| format!("Error eliminando objetos de S3: {:?}", e))?;
        }
    }
    Ok(())
}

async fn delete_cf_stack(client: &CfClient, stack_name: &str) -> Result<(), String> {
    client.delete_stack().stack_name(stack_name).send().await
        .map_err(|e| format!("Error al solicitar borrado del stack: {:?}", e))?;
    Ok(())
}