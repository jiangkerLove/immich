use std::path::Path;

use lettre::message::{header, Attachment, Body, Mailbox, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde::Deserialize;

use crate::models::response::response::ErrorResp;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpTransportConfig {
    pub ignore_cert: bool,
    pub host: String,
    pub port: u16,
    pub secure: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpConfig {
    pub enabled: bool,
    pub from: String,
    #[serde(default)]
    pub reply_to: String,
    pub transport: SmtpTransportConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailTemplate {
    Test,
    Welcome,
    AlbumInvite,
    AlbumUpdate,
}

impl EmailTemplate {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "test" => Some(Self::Test),
            "welcome" => Some(Self::Welcome),
            "album-invite" => Some(Self::AlbumInvite),
            "album-update" => Some(Self::AlbumUpdate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmailImageAttachment {
    pub filename: String,
    pub path: String,
    pub cid: String,
}

#[derive(Debug, Clone, Default)]
pub struct TestEmailData {
    pub base_url: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct WelcomeEmailData {
    pub base_url: String,
    pub display_name: String,
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AlbumInviteEmailData {
    pub base_url: String,
    pub album_name: String,
    pub album_id: String,
    pub sender_name: String,
    pub recipient_name: String,
    pub cid: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AlbumUpdateEmailData {
    pub base_url: String,
    pub album_name: String,
    pub album_id: String,
    pub recipient_name: String,
    pub cid: Option<String>,
}

pub struct RenderedEmail {
    pub html: String,
    pub text: String,
}

pub struct EmailService;

impl EmailService {
    pub async fn verify_smtp(transport: &SmtpTransportConfig) -> Result<(), ErrorResp> {
        let mailer = build_mailer(transport)?;
        mailer
            .test_connection()
            .await
            .map_err(|err| ErrorResp::BadRequest(format!("Failed to verify SMTP configuration: {err}")))?;
        Ok(())
    }

    pub async fn send(
        to: &str,
        from: &str,
        reply_to: &str,
        subject: &str,
        html: &str,
        text: &str,
        transport: &SmtpTransportConfig,
        image_attachments: &[EmailImageAttachment],
    ) -> Result<String, ErrorResp> {
        let from_mailbox: Mailbox = from
            .parse()
            .map_err(|_| ErrorResp::BadRequest("Invalid from address".to_string()))?;
        let to_mailbox: Mailbox = to
            .parse()
            .map_err(|_| ErrorResp::BadRequest("Invalid to address".to_string()))?;
        let reply_mailbox: Mailbox = reply_to
            .parse()
            .map_err(|_| ErrorResp::BadRequest("Invalid reply-to address".to_string()))?;

        let builder = Message::builder()
            .from(from_mailbox)
            .reply_to(reply_mailbox)
            .to(to_mailbox)
            .subject(subject);

        let alternative = MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(header::ContentType::TEXT_PLAIN)
                    .body(Body::new(text.to_string())),
            )
            .singlepart(
                SinglePart::builder()
                    .header(header::ContentType::TEXT_HTML)
                    .body(Body::new(html.to_string())),
            );

        let mut related = MultiPart::related().multipart(alternative);

        for attachment in image_attachments {
            if !is_supported_image_attachment(attachment) {
                continue;
            }
            let bytes = tokio::fs::read(&attachment.path)
                .await
                .map_err(|err| ErrorResp::ServerError(format!("Unable to read attachment: {err}")))?;
            let content_type = mime_guess::from_path(&attachment.path)
                .first_or_octet_stream()
                .to_string();
            related = related.singlepart(
                Attachment::new_inline(attachment.filename.clone())
                    .body(bytes, content_type.parse().unwrap()),
            );
        }

        let email = builder
            .multipart(related)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let mailer = build_mailer(transport)?;
        let response = mailer
            .send(email)
            .await
            .map_err(|err| ErrorResp::ServerError(format!("Failed to send email: {err}")))?;

        Ok(response
            .first_line()
            .unwrap_or("ok")
            .to_string())
    }

    pub fn render_test(data: &TestEmailData, _custom_template: &str) -> RenderedEmail {
        let content = format!(
            r#"<p style="margin:0;">Hey <strong>{}</strong>!</p>
<p>This is a test email from your Immich Instance!</p>
<p><a href="{base}">{base}</a></p>"#,
            html_escape(&data.display_name),
            base = html_escape(&data.base_url),
        );
        wrap_layout("This is a test email from Immich.", &content)
    }

    pub fn render_welcome(data: &WelcomeEmailData, custom_template: &str) -> RenderedEmail {
        let content = if custom_template.is_empty() {
            let password_block = data.password.as_ref().map(|password| {
                format!(
                    "<br /><strong>Password</strong>: {}",
                    html_escape(password)
                )
            }).unwrap_or_default();
            format!(
                r#"<p style="margin:0;">Hey <strong>{}</strong>!</p>
<p>A new account has been created for you.</p>
<p><strong>Username</strong>: {}{password_block}</p>
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to proceed with first login.<br />
<a href="{base}">{base}</a></p>"#,
                html_escape(&data.display_name),
                html_escape(&data.username),
                base = html_escape(&data.base_url),
                button = button("Login", &format!("{}/auth/login", data.base_url.trim_end_matches('/'))),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("displayName", data.display_name.as_str()),
                    ("username", data.username.as_str()),
                    ("password", data.password.as_deref().unwrap_or("")),
                    ("baseUrl", data.base_url.as_str()),
                ],
            );
            format!(
                r#"<div>{rendered}</div>
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to proceed with first login.<br />
<a href="{base}">{base}</a></p>"#,
                button = button("Login", &format!("{}/auth/login", data.base_url.trim_end_matches('/'))),
                base = html_escape(&data.base_url),
            )
        };

        wrap_layout("You have been invited to a new Immich instance.", &content)
    }

    pub fn render_album_invite(data: &AlbumInviteEmailData, custom_template: &str) -> RenderedEmail {
        let album_url = format!(
            "{}/albums/{}",
            data.base_url.trim_end_matches('/'),
            data.album_id
        );
        let content = if custom_template.is_empty() {
            format!(
                r#"<p style="margin:0;">Hey <strong>{}</strong>!</p>
<p>{sender} has added you to the album <strong>{album}</strong>.</p>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                html_escape(&data.recipient_name),
                sender = html_escape(&data.sender_name),
                album = html_escape(&data.album_name),
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("albumName", data.album_name.as_str()),
                    ("recipientName", data.recipient_name.as_str()),
                    ("senderName", data.sender_name.as_str()),
                    ("albumId", data.album_id.as_str()),
                    ("baseUrl", data.base_url.as_str()),
                ],
            );
            format!(
                r#"<div>{rendered}</div>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            )
        };

        wrap_layout("You have been added to a shared album.", &content)
    }

    pub fn render_album_update(data: &AlbumUpdateEmailData, custom_template: &str) -> RenderedEmail {
        let album_url = format!(
            "{}/albums/{}",
            data.base_url.trim_end_matches('/'),
            data.album_id
        );
        let content = if custom_template.is_empty() {
            format!(
                r#"<p style="margin:0;">Hey <strong>{}</strong>!</p>
<p>New media has been added to <strong>{album}</strong>.<br />Check it out!</p>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                html_escape(&data.recipient_name),
                album = html_escape(&data.album_name),
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("albumName", data.album_name.as_str()),
                    ("recipientName", data.recipient_name.as_str()),
                    ("albumId", data.album_id.as_str()),
                    ("baseUrl", data.base_url.as_str()),
                ],
            );
            format!(
                r#"<div>{rendered}</div>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            )
        };

        wrap_layout("New media has been added to a shared album.", &content)
    }
}

fn is_supported_image_attachment(attachment: &EmailImageAttachment) -> bool {
    let extension = Path::new(&attachment.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif"
    )
}

fn build_mailer(
    transport: &SmtpTransportConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, ErrorResp> {
    let tls_params = TlsParameters::builder(transport.host.clone())
        .dangerous_accept_invalid_certs(transport.ignore_cert)
        .build()
        .map_err(|err| ErrorResp::BadRequest(format!("Invalid TLS configuration: {err}")))?;

    let tls = if transport.secure {
        Tls::Wrapper(tls_params)
    } else {
        Tls::Required(tls_params)
    };

    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&transport.host)
        .map_err(|err| ErrorResp::BadRequest(format!("Invalid SMTP host: {err}")))?
        .port(transport.port)
        .tls(tls);

    if !transport.username.is_empty() || !transport.password.is_empty() {
        builder = builder.credentials(Credentials::new(
            transport.username.clone(),
            transport.password.clone(),
        ));
    }

    Ok(builder.build())
}

fn wrap_layout(preview: &str, content: &str) -> RenderedEmail {
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8" />
<title>{preview}</title>
</head>
<body style="background:#F4F4F4;font-family:Overpass,Arial,sans-serif;color:#1f2937;margin:0;padding:0;">
<div style="max-width:465px;margin:40px auto;padding:0 8px;">
  <div style="padding:48px;border:1px solid #f87171;border-radius:50px;background:#F6F6F4;">
    <img src="https://immich.app/img/immich-logo-inline-light.png" alt="Immich" style="height:48px;display:block;margin:0 auto 48px;" />
    {content}
  </div>
  <hr style="margin:8px 0;border:none;border-top:1px solid #E5E7EB;" />
  <p style="font-size:12px;color:#6A737D;text-align:center;">Powered by Immich</p>
</div>
</body>
</html>"#,
        preview = html_escape(preview),
    );

    let text = html_to_text(content);
    RenderedEmail { html, text }
}

fn button(label: &str, href: &str) -> String {
    format!(
        r#"<div style="text-align:center;margin:24px 0;">
<a href="{href}" style="background:#4250AF;color:#fff;text-decoration:none;padding:12px 24px;border-radius:999px;display:inline-block;font-weight:600;">{label}</a>
</div>"#,
        href = html_escape(href),
        label = html_escape(label),
    )
}

fn cid_image(cid: Option<&str>) -> String {
    cid.map(|value| {
        format!(
            r#"<div style="text-align:center;margin:16px 0;">
<img src="cid:{cid}" alt="Album thumbnail" style="max-width:300px;width:100%;border-radius:8px;box-shadow:rgba(50,50,93,0.25) 0 13px 27px -5px,rgba(0,0,0,0.3) 0 8px 16px -8px;" />
</div>"#,
            cid = html_escape(value),
        )
    })
    .unwrap_or_default()
}

fn replace_template_tags(template: &str, variables: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn html_to_text(content: &str) -> String {
    let mut text = content.to_string();
    for (from, to) in [
        ("<br />", "\n"),
        ("<br/>", "\n"),
        ("<br>", "\n"),
        ("</p>", "\n\n"),
        ("</div>", "\n"),
        ("</strong>", ""),
        ("<strong>", ""),
    ] {
        text = text.replace(from, to);
    }
    while text.contains("<") {
        if let Some(start) = text.find('<') {
            if let Some(end) = text[start..].find('>') {
                text.replace_range(start..start + end + 1, "");
            } else {
                break;
            }
        } else {
            break;
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_template_tags_replaces_known_keys() {
        let rendered = replace_template_tags(
            "Hello {recipientName}, welcome to {albumName}",
            &[("recipientName", "Jane"), ("albumName", "Trip")],
        );
        assert_eq!(rendered, "Hello Jane, welcome to Trip");
    }
}
