# Immich rust-server smoke test
#
# Prerequisites: rust-server running (e.g. docker compose -f docker/docker-compose.yml
#   -f docker/docker-compose.rust.yml up -d), plus a local image file.
#
# Usage:
#   $env:IMMICH_URL = "http://127.0.0.1:2283"
#   $env:IMMICH_EMAIL = "admin@example.com"
#   $env:IMMICH_PASSWORD = "..."
#   $env:SMOKE_IMAGE = "C:\path\to\photo.jpg"   # optional; creates a tiny JPEG if unset
#   .\rust-server\scripts\smoke.ps1

$ErrorActionPreference = "Stop"

$BaseUrl = ($env:IMMICH_URL ?? "http://127.0.0.1:2283").TrimEnd("/")
$Email = $env:IMMICH_EMAIL
$Password = $env:IMMICH_PASSWORD
$ImagePath = $env:SMOKE_IMAGE
$ThumbWaitSec = [int]($env:SMOKE_THUMB_WAIT_SEC ?? "90")

if (-not $Email -or -not $Password) {
    Write-Error "Set IMMICH_EMAIL and IMMICH_PASSWORD"
}

function Invoke-Immich {
    param(
        [string]$Method,
        [string]$Path,
        [hashtable]$Headers = @{},
        [object]$Body = $null,
        [string]$ContentType = "application/json",
        [string]$OutFile = $null
    )
    $uri = "$BaseUrl$Path"
    $params = @{
        Method = $Method
        Uri = $uri
        Headers = $Headers
    }
    if ($null -ne $Body) {
        if ($ContentType -eq "application/json" -and $Body -isnot [System.Net.Http.HttpContent]) {
            $params.ContentType = $ContentType
            $params.Body = if ($Body -is [string]) { $Body } else { ($Body | ConvertTo-Json -Compress -Depth 8) }
        } else {
            $params.Body = $Body
        }
    }
    if ($OutFile) {
        $params.OutFile = $OutFile
    }
    return Invoke-RestMethod @params
}

Write-Host "==> Login $Email @ $BaseUrl"
$login = Invoke-Immich -Method POST -Path "/api/auth/login" -Body @{
    email = $Email
    password = $Password
}
$token = $login.accessToken
if (-not $token) {
    Write-Error "Login failed: no accessToken"
}
$auth = @{ Authorization = "Bearer $token" }
Write-Host "    OK user=$($login.userId)"

# Tiny 1x1 JPEG if no sample provided
if (-not $ImagePath -or -not (Test-Path $ImagePath)) {
    $ImagePath = Join-Path $env:TEMP "immich-smoke.jpg"
    # Minimal valid JPEG (1x1 pixel)
    $bytes = [Convert]::FromBase64String("/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAn/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIQAxAAAAGcP//EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAQUCf//EABQRAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQMBAT8Bf//EABQRAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQIBAT8Bf//Z")
    [IO.File]::WriteAllBytes($ImagePath, $bytes)
    Write-Host "==> Using generated smoke image $ImagePath"
} else {
    Write-Host "==> Upload $ImagePath"
}

$deviceId = [guid]::NewGuid().ToString()
$assetId = [guid]::NewGuid().ToString()
$form = @{
    deviceAssetId = $deviceId
    deviceId = "smoke-ps1"
    fileCreatedAt = (Get-Date).ToUniversalTime().ToString("o")
    fileModifiedAt = (Get-Date).ToUniversalTime().ToString("o")
    assetData = Get-Item -Path $ImagePath
}

# Multipart upload via curl.exe for reliable binary field
$uploadJson = curl.exe -sS -X POST "$BaseUrl/api/assets" `
    -H "Authorization: Bearer $token" `
    -H "Accept: application/json" `
    -F "deviceAssetId=$deviceId" `
    -F "deviceId=smoke-ps1" `
    -F "fileCreatedAt=$((Get-Date).ToUniversalTime().ToString('o'))" `
    -F "fileModifiedAt=$((Get-Date).ToUniversalTime().ToString('o'))" `
    -F "assetData=@$ImagePath"
if ($LASTEXITCODE -ne 0) {
    Write-Error "Upload curl failed (exit $LASTEXITCODE)"
}
$upload = $uploadJson | ConvertFrom-Json
$assetId = $upload.id
if (-not $assetId) {
    Write-Error "Upload failed: $uploadJson"
}
Write-Host "    OK assetId=$assetId status=$($upload.status)"

Write-Host "==> Wait for thumbnail (up to ${ThumbWaitSec}s)"
$deadline = (Get-Date).AddSeconds($ThumbWaitSec)
$ready = $false
while ((Get-Date) -lt $deadline) {
    try {
        $null = Invoke-WebRequest -Method GET -Uri "$BaseUrl/api/assets/$assetId/thumbnail?size=thumbnail" `
            -Headers $auth -TimeoutSec 10
        $ready = $true
        break
    } catch {
        Start-Sleep -Seconds 2
    }
}
if (-not $ready) {
    Write-Error "Thumbnail not ready within ${ThumbWaitSec}s"
}
Write-Host "    OK thumbnail"

Write-Host "==> Search metadata"
$search = Invoke-Immich -Method POST -Path "/api/search/metadata" -Headers $auth -Body @{
    size = 10
}
$count = @($search.assets.items).Count
Write-Host "    OK search returned $count item(s)"

Write-Host "==> Sync stream (AuthUserV1)"
$session = Invoke-Immich -Method POST -Path "/api/sessions" -Headers $auth -Body @{
    deviceType = "smoke"
    deviceOS = "windows"
}
# Sync uses session cookie/token from login; stream with types
try {
    $syncBody = @{ types = @("AuthUserV1") } | ConvertTo-Json -Compress
    $syncResp = Invoke-WebRequest -Method POST -Uri "$BaseUrl/api/sync/stream" `
        -Headers ($auth + @{ "Content-Type" = "application/json"; "Accept" = "application/jsonlines" }) `
        -Body $syncBody -TimeoutSec 30
    Write-Host "    OK sync stream status=$($syncResp.StatusCode) bytes=$($syncResp.RawContentLength)"
} catch {
    Write-Host "    WARN sync stream: $($_.Exception.Message) (non-fatal if session setup differs)"
}

Write-Host ""
Write-Host "Smoke passed: login → upload → thumbnail → search"
