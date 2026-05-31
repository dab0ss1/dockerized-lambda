use lambda_models::LambdaRequest;
use hyper::{Request, body::Incoming};
use http_body_util::BodyExt;
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;
use super::error::ConvertError;

/// Convert incoming HTTP request to LambdaRequest for sending to container
#[tracing::instrument(name = "HttpToLambdaRequest", skip_all)]
pub async fn http_to_lambda_request(req: Request<Incoming>) -> Result<LambdaRequest, ConvertError> {
    let (parts, body) = req.into_parts();

    // Extract method
    let method = parts.method;

    // Extract path
    let path = parts.uri.path().to_string();

    // Extract query parameters
    let query_parameters: HashMap<String, String> = parts
        .uri
        .query()
        .map(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default();

    // Extract headers - forward all headers (as requested)
    let headers: HashMap<String, String> = parts
        .headers
        .iter()
        .filter_map(|(name, value)| {
            match value.to_str() {
                Ok(value_str) => Some((name.to_string(), value_str.to_string())),
                Err(_) => {
                    tracing::warn!("Skipping header with invalid UTF-8: {}", name);
                    None
                }
            }
        })
        .collect();

    // Read body (buffered approach)
    let body_bytes = body.collect().await
        .map_err(|e| ConvertError::BodyRead(e.to_string()))?
        .to_bytes();

    let body_string = String::from_utf8(body_bytes.to_vec())
        .map_err(|_| ConvertError::InvalidUtf8)?;

    // Extract remote address (if available from extensions)
    let remote_addr = parts
        .extensions
        .get::<std::net::SocketAddr>()
        .map(|addr| addr.ip());

    Ok(LambdaRequest {
        request_id: Uuid::new_v4(),
        method,
        path,
        query_parameters,
        headers,
        body: body_string,
        remote_addr,
        timestamp: Utc::now(),
    })
}
