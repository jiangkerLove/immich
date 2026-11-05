use lettre::message::{Attachment, Body, Mailbox, Message, MultiPart, SinglePart, header};
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
        mailer.test_connection().await.map_err(|err| {
            ErrorResp::BadRequest(format!("Failed to verify SMTP configuration: {err}"))
        })?;
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
            // TS attaches any extension; keep the HTML cid in sync by always attaching.
            let bytes = tokio::fs::read(&attachment.path).await.map_err(|err| {
                ErrorResp::ServerError(format!("Unable to read attachment: {err}"))
            })?;
            let content_type = mime_guess::from_path(&attachment.path)
                .first_or_octet_stream()
                .to_string();
            // Lettre treats this argument as Content-ID; match nodemailer cid: album-thumbnail.
            related = related.singlepart(
                Attachment::new_inline(attachment.cid.clone())
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

        Ok(response.first_line().unwrap_or("ok").to_string())
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
        let (preview, content) = if custom_template.is_empty() {
            // Match TS `password &&`: empty string omits the password line.
            let password_block = data
                .password
                .as_ref()
                .filter(|password| !password.is_empty())
                .map(|password| {
                    format!("<br /><strong>Password</strong>: {}", html_escape(password))
                })
                .unwrap_or_default();
            (
                "You have been invited to a new Immich instance.".to_string(),
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
                    button = button(
                        "Login",
                        &format!("{}/auth/login", data.base_url.trim_end_matches('/'))
                    ),
                ),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("displayName", Some(data.display_name.as_str())),
                    ("username", Some(data.username.as_str())),
                    (
                        "password",
                        data.password.as_deref().filter(|value| !value.is_empty()),
                    ),
                    ("baseUrl", Some(data.base_url.as_str())),
                ],
            );
            // Match TS: custom template body is also the Preview string.
            let preview = rendered.clone();
            let content = format!(
                r#"<div>{rendered}</div>
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to proceed with first login.<br />
<a href="{base}">{base}</a></p>"#,
                button = button(
                    "Login",
                    &format!("{}/auth/login", data.base_url.trim_end_matches('/'))
                ),
                base = html_escape(&data.base_url),
            );
            (preview, content)
        };

        wrap_layout(&preview, &content)
    }

    pub fn render_album_invite(
        data: &AlbumInviteEmailData,
        custom_template: &str,
    ) -> RenderedEmail {
        let album_url = format!(
            "{}/albums/{}",
            data.base_url.trim_end_matches('/'),
            data.album_id
        );
        let (preview, content) = if custom_template.is_empty() {
            (
                "You have been added to a shared album.".to_string(),
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
                ),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("albumName", nonempty(&data.album_name)),
                    ("recipientName", nonempty(&data.recipient_name)),
                    ("senderName", nonempty(&data.sender_name)),
                    ("albumId", nonempty(&data.album_id)),
                    ("baseUrl", nonempty(&data.base_url)),
                ],
            );
            let preview = rendered.clone();
            let content = format!(
                r#"<div>{rendered}</div>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            );
            (preview, content)
        };

        wrap_layout(&preview, &content)
    }

    pub fn render_album_update(
        data: &AlbumUpdateEmailData,
        custom_template: &str,
    ) -> RenderedEmail {
        let album_url = format!(
            "{}/albums/{}",
            data.base_url.trim_end_matches('/'),
            data.album_id
        );
        let (preview, content) = if custom_template.is_empty() {
            (
                "New media has been added to a shared album.".to_string(),
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
                ),
            )
        } else {
            let rendered = replace_template_tags(
                custom_template,
                &[
                    ("albumName", nonempty(&data.album_name)),
                    ("recipientName", nonempty(&data.recipient_name)),
                    ("albumId", nonempty(&data.album_id)),
                    ("baseUrl", nonempty(&data.base_url)),
                ],
            );
            let preview = rendered.clone();
            let content = format!(
                r#"<div>{rendered}</div>
{image}
{button}
<p style="font-size:12px;">If you cannot click the button use the link below to view the album.<br />
<a href="{album_url}">{album_url}</a></p>"#,
                image = cid_image(data.cid.as_deref()),
                button = button("View Album", &album_url),
                album_url = html_escape(&album_url),
            );
            (preview, content)
        };

        wrap_layout(&preview, &content)
    }
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
</head>
<body style="background:#F4F4F4;font-family:Overpass,Arial,sans-serif;color:#1f2937;margin:0;padding:0;">
{preheader}
<div style="max-width:465px;margin:40px auto;padding:0 8px;">
  <div style="padding:48px;border:1px solid #f87171;border-radius:50px;background:#F6F6F4;">
    <img src="https://immich.app/img/immich-logo-inline-light.png" alt="Immich" style="height:48px;display:block;margin:0 auto 48px;" />
    {content}
  </div>
  <hr style="margin:8px 0;border:none;border-top:1px solid #E5E7EB;" />
  {footer}
</div>
</body>
</html>"#,
        preheader = render_preheader(preview),
        footer = immich_footer_html(),
    );

    // Match react-email: plain text is derived from the finished HTML document.
    let text = html_to_text(&html);
    RenderedEmail { html, text }
}

/// Match `@react-email/preview`: hidden inbox snippet + padding so Gmail does not
/// append body text.
fn render_preheader(preview: &str) -> String {
    let escaped = html_escape(preview);
    let padding_needed = 150usize.saturating_sub(preview.chars().count());
    let padding: String = std::iter::repeat("\u{00a0}\u{200c}")
        .take(padding_needed)
        .collect();
    format!(
        r#"<div style="display:none;overflow:hidden;line-height:1px;opacity:0;max-height:0;max-width:0" data-skip-in-text="true">{escaped}<div>{padding}</div></div>"#
    )
}

fn immich_footer_html() -> String {
    r#"<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="width:100%;">
  <tr>
    <td align="center" style="width:50%;padding:8px;">
      <a href="https://play.google.com/store/apps/details?id=app.alextran.immich">
        <img src="https://immich.app/img/google-play-badge.png" alt="Get it on Google Play" style="max-width:100%;height:auto;display:block;margin:0 auto;" />
      </a>
    </td>
    <td align="center" style="width:50%;padding:24px 8px;">
      <a href="https://apps.apple.com/sg/app/immich/id1613945652">
        <img src="https://immich.app/img/ios-app-store-badge.png" alt="Download on the App Store" style="max-width:100%;height:auto;display:block;margin:0 auto;" />
      </a>
    </td>
  </tr>
</table>
<p style="font-size:14px;line-height:20px;color:#6A737D;text-align:center;margin:16px 0;">
  <a href="https://immich.app" style="color:#6A737D;">Immich</a> project is available under GNU AGPL v3 license.
</p>"#
        .to_string()
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

fn nonempty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

/// Match TS `/{(.*?)}/g`: one pass; missing/empty values leave `{key}` in place.
fn replace_template_tags(template: &str, variables: &[(&str, Option<&str>)]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            result.push('{');
            rest = after;
            continue;
        };
        let key = &after[..end];
        let replacement = variables
            .iter()
            .find(|(name, value)| *name == key && value.is_some_and(|v| !v.is_empty()))
            .and_then(|(_, value)| *value);
        if let Some(value) = replacement {
            result.push_str(value);
        } else {
            result.push('{');
            result.push_str(key);
            result.push('}');
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
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

/// Approximate `@react-email/render` plainText / html-to-text 9.x for Immich layouts.
fn html_to_text(html: &str) -> String {
    let mut body = html.to_string();
    if let Some(start) = body.find("<body") {
        if let Some(gt) = body[start..].find('>') {
            body = body[start + gt + 1..].to_string();
        }
    }
    if let Some(end) = body.rfind("</body>") {
        body.truncate(end);
    }

    // Drop preheader and images (badge links keep their href via <a> handling).
    body = strip_tagged_blocks(&body, "data-skip-in-text");
    body = strip_void_tags(&body, "img");

    let mut out = String::new();
    let mut rest = body.as_str();
    while let Some(start) = rest.find('<') {
        push_decoded_text(&mut out, &rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            push_decoded_text(&mut out, rest);
            break;
        };
        let tag = &after[..end];
        let lower = tag.to_ascii_lowercase();
        let name = lower
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let is_close = lower.starts_with('/');

        match (name, is_close) {
            ("br", _) => out.push('\n'),
            ("hr", _) => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("------------------------------------------------------------\n");
            }
            ("p" | "div" | "tr" | "table" | "section", true) => {
                if !out.ends_with("\n\n") {
                    if out.ends_with('\n') {
                        out.push('\n');
                    } else {
                        out.push_str("\n\n");
                    }
                }
            }
            ("a", false) => {
                let href = attr_value(tag, "href").unwrap_or_default();
                let close = after[end + 1..]
                    .find("</a>")
                    .or_else(|| after[end + 1..].find("</A>"));
                if let Some(rel) = close {
                    let inner = &after[end + 1..end + 1 + rel];
                    let mut label = String::new();
                    html_to_text_fragment(inner, &mut label);
                    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
                    if label.is_empty() || label == href {
                        out.push_str(&href);
                    } else {
                        out.push_str(&label);
                        out.push(' ');
                        out.push_str(&href);
                    }
                    rest = &after[end + 1 + rel + 4..];
                    continue;
                }
            }
            _ => {}
        }
        rest = &after[end + 1..];
    }
    push_decoded_text(&mut out, rest);

    normalize_plain_text(&out)
}

fn html_to_text_fragment(fragment: &str, out: &mut String) {
    let mut rest = fragment;
    while let Some(start) = rest.find('<') {
        push_decoded_text(out, &rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('>') {
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    push_decoded_text(out, rest);
}

fn strip_tagged_blocks(html: &str, marker: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(idx) = rest.find(marker) {
        let open = rest[..idx].rfind('<').unwrap_or(0);
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        if let Some(gt) = after_open.find('>') {
            let tag_name = after_open[1..gt]
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("div");
            let close = format!("</{tag_name}>");
            if let Some(rel) = after_open[gt + 1..].to_ascii_lowercase().find(&close) {
                rest = &after_open[gt + 1 + rel + close.len()..];
                continue;
            }
        }
        out.push_str(&rest[open..open + 1]);
        rest = &rest[open + 1..];
    }
    out.push_str(rest);
    out
}

fn strip_void_tags(html: &str, tag: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let open = format!("<{tag}");
    while let Some(idx) = rest.to_ascii_lowercase().find(&open) {
        out.push_str(&rest[..idx]);
        if let Some(end) = rest[idx..].find('>') {
            rest = &rest[idx + end + 1..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let idx = lower.find(&key)?;
    let after = &tag[idx + key.len()..];
    let quote = after.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = after[1..].find(quote)?;
        Some(html_unescape(&after[1..1 + end]))
    } else {
        Some(
            after
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_string(),
        )
    }
}

fn push_decoded_text(out: &mut String, value: &str) {
    out.push_str(&html_unescape(value));
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn normalize_plain_text(value: &str) -> String {
    let mut lines = Vec::new();
    for line in value.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        lines.push(trimmed);
    }
    let mut text = lines.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_template_tags_replaces_known_keys() {
        let rendered = replace_template_tags(
            "Hello {recipientName}, welcome to {albumName}",
            &[("recipientName", Some("Jane")), ("albumName", Some("Trip"))],
        );
        assert_eq!(rendered, "Hello Jane, welcome to Trip");
    }

    #[test]
    fn replace_template_tags_keeps_empty_and_missing_keys() {
        let rendered = replace_template_tags(
            "Hey {recipientName}! {password}",
            &[("recipientName", Some("Jane")), ("password", None)],
        );
        assert_eq!(rendered, "Hey Jane! {password}");
    }

    #[test]
    fn layout_includes_store_badges_preheader_and_agpl_footer() {
        let rendered = EmailService::render_test(
            &TestEmailData {
                base_url: "https://my.immich.app".into(),
                display_name: "Alan".into(),
            },
            "",
        );
        assert!(rendered.html.contains("data-skip-in-text=\"true\""));
        assert!(rendered.html.contains("This is a test email from Immich."));
        assert!(rendered.html.contains("google-play-badge.png"));
        assert!(rendered.html.contains("ios-app-store-badge.png"));
        assert!(rendered.html.contains("GNU AGPL v3 license"));
        assert!(!rendered.html.contains("Powered by Immich"));
        assert!(rendered.text.contains("https://my.immich.app"));
        assert!(rendered.text.contains("https://play.google.com/store/apps/details?id=app.alextran.immich"));
        assert!(rendered
            .text
            .contains("Immich https://immich.app project is available under GNU AGPL v3 license."));
        assert!(!rendered.text.contains("This is a test email from Immich."));
    }

    #[test]
    fn custom_welcome_template_becomes_preview() {
        let rendered = EmailService::render_welcome(
            &WelcomeEmailData {
                base_url: "https://demo.immich.app".into(),
                display_name: "Ada".into(),
                username: "ada@example.com".into(),
                password: Some("secret".into()),
            },
            "Welcome {displayName}",
        );
        assert!(rendered.html.contains("data-skip-in-text=\"true\">Welcome Ada"));
    }
}
