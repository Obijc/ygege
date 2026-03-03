use crate::DOMAIN;
use crate::config::Config;
use crate::rest::client_extractor::MaybeCustomClient;
use crate::flaresolverr::{FLARESOLVERR, FlareSolverrClient, FlareSolverrCookie};
use actix_web::{HttpRequest, HttpResponse, get, web};
use serde_json::Value;
use tokio::time::{Duration, sleep};
use wreq::header::HeaderMap;

#[get("/torrent/{id:[0-9]+}")]
pub async fn download_torrent(
    data: MaybeCustomClient,
    config: web::Data<Config>,
    req_data: HttpRequest,
) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let id = req_data.match_info().get("id").unwrap();
    let id = id.parse::<usize>()?;

    let domain_lock = DOMAIN.lock()?;
    let cloned_guard = domain_lock.clone();
    let domain = cloned_guard.as_str();
    drop(domain_lock);

    let token_url = format!("https://{}/engine/start_download_timer", domain);
    let body_str = format!("torrent_id={}", id);

    debug!("Request download token {} torrent_id={}", token_url, id);

    // --- Step 1: Get CF cookies from FlareSolverr if needed ---
    // We need valid CF cookies to make direct wreq requests.
    // Strategy: try wreq first; if CF blocks, use FlareSolverr to get cookies, then retry wreq.

    let (token, cf_cookies, cf_ua) = get_download_token(
        &data.client,
        &token_url,
        &body_str,
    ).await?;

    debug!("Token response: {}", token);

    // --- Step 2: Wait ---
    let wait_secs = if config.turbo_enabled.unwrap_or(false) {
        5
    } else {
        35
    };
    debug!("Wait {} secs...", wait_secs);
    sleep(Duration::from_secs(wait_secs)).await;
    debug!("Wait is over");

    // --- Step 3: Download the signed torrent file ---
    let download_url = format!(
        "https://{}/engine/download_torrent?id={}&token={}",
        domain, id, token
    );
    debug!("download URL {}", download_url);

    let torrent_bytes = download_torrent_binary(
        &data.client,
        &download_url,
        cf_cookies.as_deref(),
        cf_ua.as_deref(),
        domain,
    ).await?;

    if torrent_bytes.is_empty() {
        return Err("Downloaded torrent file is empty".into());
    }

    // Validate: bencoded torrent files start with "d"
    if torrent_bytes.first() != Some(&b'd') {
        let preview = String::from_utf8_lossy(&torrent_bytes[..torrent_bytes.len().min(100)]);
        error!("Downloaded content does not look like a torrent file (starts with: {:?})", &preview);
        return Err("Downloaded content is not a valid torrent file".into());
    }

    debug!("Torrent file size: {} bytes", torrent_bytes.len());

    let mut response_builder = HttpResponse::Ok();
    response_builder
        .content_type("application/x-bittorrent")
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}.torrent\"", id),
        ));

    if let Some(cookies) = data.cookies_header {
        response_builder.insert_header(("X-Session-Cookies", cookies));
    }

    Ok(response_builder.body(torrent_bytes))
}

/// Request the download token.
/// Returns (token, Option<cookies_string>, Option<user_agent>).
/// If CF blocks wreq, uses FlareSolverr's request.post to make the POST
/// through a real browser, bypassing CF completely.
async fn get_download_token(
    client: &wreq::Client,
    url: &str,
    post_data: &str,
) -> Result<(String, Option<String>, Option<String>), Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8".parse().unwrap());
    headers.insert("X-Requested-With", "XMLHttpRequest".parse().unwrap());

    // Try direct wreq POST first
    match client
        .post(url)
        .headers(headers.clone())
        .body(post_data.to_string())
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let text = response.text().await?;
            let token = extract_token_from_json(&text)?;
            return Ok((token, None, None));
        }
        Ok(response) => {
            let status = response.status();
            warn!("wreq POST request failed (HTTP {}) for {} — trying FlareSolverr request.post", status, url);
        }
        Err(e) => {
            warn!("wreq POST request failed for {}: {} — trying FlareSolverr request.post", url, e);
        }
    }

    // FlareSolverr fallback: use request.post to POST through a real browser
    if !FlareSolverrClient::is_available() {
        return Err("CF blocked token request and FlareSolverr not configured".into());
    }

    let fs = FLARESOLVERR
        .get()
        .ok_or("FlareSolverr not configured")?;

    let session_id = match FlareSolverrClient::get_session_id().await {
        Some(id) => id,
        None => {
            info!("No FlareSolverr session exists, creating one for token POST...");
            fs.create_session().await?
        }
    };

    info!("FlareSolverr: posting to {} via request.post", url);
    let solution = fs
        .solve_post(url, post_data, None, 60000, Some(&session_id))
        .await?;

    // Extract token from the FlareSolverr response (HTML body containing JSON)
    let token = extract_token_from_json(&solution.response)?;

    // Build cookie header from the solution for subsequent wreq requests (binary download)
    let domain = url
        .trim_start_matches("https://")
        .splitn(2, '/')
        .next()
        .unwrap_or("www.yggtorrent.org");
    let cookie_header = build_cookie_header(&solution.cookies, domain);

    debug!("FlareSolverr request.post succeeded, got token and {} cookies", solution.cookies.len());

    Ok((token, Some(cookie_header), Some(solution.user_agent.clone())))
}

/// Download the torrent binary file using wreq.
/// If CF cookies are available, uses them directly.
/// Otherwise tries wreq first, then gets cookies from FlareSolverr and retries.
async fn download_torrent_binary(
    client: &wreq::Client,
    url: &str,
    cf_cookies: Option<&str>,
    cf_ua: Option<&str>,
    domain: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Build headers with CF credentials if we have them
    let mut headers = HeaderMap::new();
    if let Some(cookies) = cf_cookies {
        headers.insert("Cookie", cookies.parse().unwrap());
    }
    if let Some(ua) = cf_ua {
        headers.insert("User-Agent", ua.parse().unwrap());
    }
    headers.insert("Referer", format!("https://{}/", domain).parse().unwrap());

    // Try to download using a basic wreq client (without JA3 TLS emulation)
    // Cloudflare blocks requests where the User-Agent (from FlareSolverr) doesn't
    // match the JA3 TLS fingerprint (Chrome132 forced by the shared wreq client).
    // Using a plain client avoids this mismatch.
    let basic_client = wreq::Client::builder()
        .gzip(true)
        .deflate(true)
        .brotli(true)
        .zstd(true)
        .cookie_store(true)
        // Keep DNS logic if necessary, but basic uses system DNS by default
        .cert_verification(false)
        .verify_hostname(false)
        .build()?;

    // Try downloading with the basic client
    match basic_client.get(url).headers(headers.clone()).send().await {
        Ok(response) if response.status().is_success() => {
            debug!("Basic wreq download succeeded (bypassed CF TLS check)");
            return Ok(response.bytes().await?.to_vec());
        }
        Ok(response) => {
            let status = response.status();
            warn!("Basic wreq download failed (HTTP {})", status);

            if status.as_u16() == 302 {
                return match crate::utils::get_remaining_downloads(&basic_client).await {
                    Ok(0) => {
                        error!("No remaining downloads");
                        Err("No remaining downloads".into())
                    }
                    Ok(n) => {
                        warn!("Failed to download torrent, {} remaining downloads", n);
                        Err("Failed to download torrent, but you have remaining downloads.".into())
                    }
                    Err(e) => {
                        error!("Error checking remaining downloads: {}", e);
                        Err("Failed to download torrent and check remaining downloads.".into())
                    }
                };
            }
        }
        Err(e) => {
            warn!("Basic wreq download error: {}", e);
        }
    }

    // FlareSolverr fallback: use request.get through a real browser
    warn!("wreq download failed, trying FlareSolverr GET fallback for {}", url);
    if !FlareSolverrClient::is_available() {
        return Err("Torrent download failed and FlareSolverr not configured".into());
    }

    let solution = FlareSolverrClient::fetch_page_with_solution(url).await?;
    let body = &solution.response;

    // Try base64 decode first (binary responses may be encoded)
    use base64::{Engine as _, engine::general_purpose};
    if let Ok(bytes) = general_purpose::STANDARD.decode(body) {
        if !bytes.is_empty() {
            info!("FlareSolverr: decoded torrent binary from base64 ({} bytes)", bytes.len());
            return Ok(bytes);
        }
    }

    // Otherwise treat the response as raw bytes
    let bytes = body.as_bytes().to_vec();
    if bytes.is_empty() {
        return Err("FlareSolverr returned empty response for torrent download".into());
    }
    info!("FlareSolverr: using raw response as torrent binary ({} bytes)", bytes.len());
    Ok(bytes)
}

/// Extract token from JSON response (handles both plain JSON and HTML-wrapped JSON from FlareSolverr)
fn extract_token_from_json(raw: &str) -> Result<String, Box<dyn std::error::Error>> {
    let json_str = if raw.contains('{') && raw.contains('}') {
        let start = raw.find('{').unwrap();
        let end = raw.rfind('}').unwrap();
        &raw[start..=end]
    } else {
        raw
    };

    let body: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse token JSON: {} (raw: {:?})", e, &raw[..raw.len().min(200)]))?;

    body.get("token")
        .and_then(|h| h.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Token not found in start_download_timer response".into())
}

/// Build a Cookie header string from FlareSolverr cookies, filtering to the target domain
fn build_cookie_header(cookies: &[FlareSolverrCookie], domain: &str) -> String {
    cookies
        .iter()
        .filter(|c| domain.contains(c.domain.trim_start_matches('.')))
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}
