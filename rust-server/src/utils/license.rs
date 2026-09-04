use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::RsaPublicKey;
use rsa::pkcs1v15::VerifyingKey;
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use sha2::Sha256;

const STAGING_SERVER_KEY: &str = "LS0tLS1CRUdJTiBQVUJMSUMgS0VZLS0tLS0KTUlJQklqQU5CZ2txaGtpRzl3MEJBUUVGQUFPQ0FROEFNSUlCQ2dLQ0FRRUE3Sy8yd3ZLUS9NdU8ydi9MUm5saAoyUy9zTHhDOGJiTEw1UUlKOGowQ3BVZW40YURlY2dYMUpKUmtGNlpUVUtpNTdTbEhtS3RSM2JOTzJmdTBUUVg5Ck5WMEJzVzllZVB0MmlTMWl4VVFmTzRObjdvTjZzbEtac01qd29RNGtGRGFmM3VHTlZJc0dMb3UxVWRLUVhpeDEKUlRHcXVTb3NZVjNWRlk3Q1hGYTVWaENBL3poVXNsNGFuVXp3eEF6M01jUFVlTXBaenYvbVZiQlRKVzBPSytWZgpWQUJvMXdYMkVBanpBekVHVzQ3Vko4czhnMnQrNHNPaHFBNStMQjBKVzlORUg5QUpweGZzWE4zSzVtM00yNUJVClZXcTlRYStIdHRENnJ0bnAvcUFweXVkWUdwZk9HYTRCUlZTR1MxMURZM0xrb2FlRzYwUEU5NHpoYjduOHpMWkgKelFJREFRQUIKLS0tLS1FTkQgUFVCTElDIEtFWS0tLS0tDQo=";

const PRODUCTION_SERVER_KEY: &str = "LS0tLS1CRUdJTiBQVUJMSUMgS0VZLS0tLS0KTUlJQklqQU5CZ2txaGtpRzl3MEJBUUVGQUFPQ0FROEFNSUlCQ2dLQ0FRRUFvcG5ZRGEwYS9kVTVJZUc3NGlFRQpNd2RBS2pzTmN6TGRDcVJkMVo5eTVUMndqTzdlWUlPZUpUc2wzNTBzUjBwNEtmU1VEU1h2QzlOcERwYzF0T0tsCjVzaEMvQXhwdlFBTENva0Y0anQ4dnJyZDlmQ2FYYzFUcVJiT21uaGl1Z0Q2dmtyME8vRmIzVURpM1UwVHZoUFAKbFBkdlNhd3pMcldaUExmbUhWVnJiclNLbW45SWVTZ3kwN3VrV1RJeUxzY2lOcnZuQnl3c0phUmVEdW9OV1BCSApVL21vMm1YYThtNHdNV2hpWGVoaUlPUXFNdVNVZ1BlQ3NXajhVVngxQ0dsUnpQREEwYlZOUXZlS1hXVnhjRUk2ClVMRWdKeTJGNDlsSDArYVlDbUJmN05FcjZWUTJXQjk1ZXZUS1hLdm4wcUlNN25nRmxjVUF3NmZ1VjFjTkNUSlMKNndJREFRQUIKLS0tLS1FTkQgUFVCTElDIEtFWS0tLS0tDQo=";

const STAGING_CLIENT_KEY: &str = "LS0tLS1CRUdJTiBQVUJMSUMgS0VZLS0tLS0KTUlJQklqQU5CZ2txaGtpRzl3MEJBUUVGQUFPQ0FROEFNSUlCQ2dLQ0FRRUFuSUNyTm5jbGpPSC9JdTNtWVVaRQp0dGJLV1c3OGRuajl5M0U2ekk3dU1NUndEckdYWFhkTGhkUDFxSWtlZHh0clVVeUpCMWR4R04yQW91S082MlNGCldrbU9PTmNGQlRBWFZTdjhUNVY0S0VwWnFQYWEwaXpNaGxMaE5sRXEvY1ZKdllrWlh1Z2x6b1o3cG1nbzFSdHgKam1iRm5NNzhrYTFRUUJqOVdLaEw2eWpWRUl2MDdVS0lKWHBNTnNuS2g1V083MjZhYmMzSE9udTlETjY5VnFFRQo3dGZrUnRWNmx2U1NzMkFVMngzT255cHA4ek53b0lPTWRibGsyb09aWWROZzY0Y3l2SzJoU0FlU3NVMFRyOVc5Ckgra0Y5QlNCNlk0QXl0QlVkSmkrK2pMSW5HM2Q5cU9ieFVzTlYrN05mRkF5NjJkL0xNR0xSOC9OUFc0U0s3c0MKRlFJREFRQUIKLS0tLS1FTkQgUFVCTElDIEtFWS0tLS0tDQo=";

const PRODUCTION_CLIENT_KEY: &str = "LS0tLS1CRUdJTiBQVUJMSUMgS0VZLS0tLS0KTUlJQklqQU5CZ2txaGtpRzl3MEJBUUVGQUFPQ0FROEFNSUlCQ2dLQ0FRRUF2LzdTMzJjUkE1KysxTm5WRHNDTQpzcFAvakpISU1xT0pYRm5oNE53QTJPcHorUk1mZGNvOTJQc09naCt3d1FlRXYxVTJjMnBqelRpUS8ybHJLcS9rCnpKUmxYd2M0Y1Vlc1FETUpPRitQMnFPTlBiQUprWHZDWFlCVUxpdENJa29Md2ZoU0dOanlJS2FSRGhkL3ROeU4KOCtoTlJabllUMWhTSWo5U0NrS3hVQ096YXRQVjRtQ0RlclMrYkUrZ0VVZVdwOTlWOWF6dkYwRkltblRXcFFTdwpjOHdFWmdPTWg0c3ZoNmFpY3dkemtQQ3dFTGFrMFZhQkgzMUJFVUNRTGI5K0FJdEhBVXRKQ0t4aGI1V2pzMXM5CmJyWGZpMHZycGdjWi82RGFuWTJxZlNQem5PbXZEMkZycmxTMXE0SkpOM1ZvN1d3LzBZeS95TWNtelRXWmhHdWgKVVFJREFRQUIKLS0tLS1FTkQgUFVCTElDIEtFWS0tLS0tDQo=";

fn server_public_key_pem() -> &'static str {
    match std::env::var("IMMICH_ENV").as_deref() {
        Ok("production") | Ok("prod") => PRODUCTION_SERVER_KEY,
        _ => STAGING_SERVER_KEY,
    }
}

fn client_public_key_pem() -> &'static str {
    match std::env::var("IMMICH_ENV").as_deref() {
        Ok("production") | Ok("prod") => PRODUCTION_CLIENT_KEY,
        _ => STAGING_CLIENT_KEY,
    }
}

fn verify_with_key(license_key: &str, activation_key: &str, public_key_b64: &str) -> bool {
    let Ok(pem_bytes) = STANDARD.decode(public_key_b64) else {
        return false;
    };
    let Ok(pem) = std::str::from_utf8(&pem_bytes) else {
        return false;
    };
    let Ok(public_key) = RsaPublicKey::from_public_key_pem(pem) else {
        return false;
    };
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let Ok(signature) = STANDARD.decode(activation_key) else {
        return false;
    };
    let Ok(signature) = rsa::pkcs1v15::Signature::try_from(signature.as_slice()) else {
        return false;
    };
    verifying_key
        .verify(license_key.as_bytes(), &signature)
        .is_ok()
}

pub fn verify_server_license(license_key: &str, activation_key: &str) -> bool {
    verify_with_key(license_key, activation_key, server_public_key_pem())
}

pub fn verify_user_license(license_key: &str, activation_key: &str) -> bool {
    verify_with_key(license_key, activation_key, client_public_key_pem())
        || verify_with_key(license_key, activation_key, server_public_key_pem())
}

pub fn is_valid_server_license_prefix(license_key: &str) -> bool {
    license_key.starts_with("IMSV-")
}

pub fn is_valid_user_license_prefix(license_key: &str) -> bool {
    license_key.starts_with("IMCL-") || license_key.starts_with("IMSV-")
}
