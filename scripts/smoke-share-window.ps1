param(
  [int]$Port = 9223,
  [switch]$LaunchBrowser
)

$ErrorActionPreference = "Stop"

function Get-CdpTargets {
  $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/json/list" -TimeoutSec 3
  return @($response.Content | ConvertFrom-Json)
}

function Connect-CdpSocket([string]$Url) {
  $socket = [Net.WebSockets.ClientWebSocket]::new()
  $null = $socket.ConnectAsync(
    [Uri]$Url,
    [Threading.CancellationToken]::None
  ).GetAwaiter().GetResult()
  return $socket
}

function Invoke-Cdp(
  [Net.WebSockets.ClientWebSocket]$Socket,
  [ref]$Sequence,
  [string]$Method,
  [hashtable]$Parameters = @{}
) {
  $Sequence.Value += 1
  $id = $Sequence.Value
  $request = @{
    id = $id
    method = $Method
    params = $Parameters
  } | ConvertTo-Json -Compress -Depth 12
  $bytes = [Text.Encoding]::UTF8.GetBytes($request)
  $segment = [ArraySegment[byte]]::new($bytes)
  $null = $Socket.SendAsync(
    $segment,
    [Net.WebSockets.WebSocketMessageType]::Text,
    $true,
    [Threading.CancellationToken]::None
  ).GetAwaiter().GetResult()

  while ($true) {
    $stream = [IO.MemoryStream]::new()
    do {
      $buffer = [byte[]]::new(65536)
      $received = $Socket.ReceiveAsync(
        [ArraySegment[byte]]::new($buffer),
        [Threading.CancellationToken]::None
      ).GetAwaiter().GetResult()
      if ($received.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) {
        throw "CDP connection closed while waiting for $Method."
      }
      $stream.Write($buffer, 0, $received.Count)
    } while (-not $received.EndOfMessage)
    $message = [Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json
    $stream.Dispose()
    if ($message.id -eq $id) {
      if ($message.error) {
        throw "CDP $Method failed: $($message.error.message)"
      }
      return $message.result
    }
  }
}

$targets = Get-CdpTargets
$mainTarget = $targets | Where-Object {
  $_.type -eq "page" -and $_.title -eq "Purple Capture" -and $_.url -notlike "*share.html*"
} | Select-Object -First 1
if (-not $mainTarget) {
  throw "Purple Capture main WebView target was not found."
}

$mainSocket = Connect-CdpSocket $mainTarget.webSocketDebuggerUrl
$mainSequence = 0
try {
  Invoke-Cdp $mainSocket ([ref]$mainSequence) "Runtime.enable" | Out-Null
  $layoutResult = Invoke-Cdp $mainSocket ([ref]$mainSequence) "Runtime.evaluate" @{
    expression = "(()=>{const removed=!document.querySelector('[data-page=""browser""]')&&!document.querySelector('#page-browser');const panel=document.querySelector('#share-panel');const tab=document.querySelector('[data-source-type=""browser""]');tab?.click();return {removed,hasPanel:!!panel,hasTab:!!tab};})()"
    returnByValue = $true
  }
  $layout = $layoutResult.result.value
  if (-not $layout.removed -or -not $layout.hasPanel -or -not $layout.hasTab) {
    throw "Recording-page share layout is incomplete or the removed browser menu remains."
  }
  Invoke-Cdp $mainSocket ([ref]$mainSequence) "Runtime.evaluate" @{
    expression = "document.querySelector('[data-source-type=""browser""]')?.click()"
    returnByValue = $true
  } | Out-Null
  Start-Sleep -Milliseconds 500

  $buttonResult = Invoke-Cdp $mainSocket ([ref]$mainSequence) "Runtime.evaluate" @{
    expression = "(()=>{const b=document.querySelector('#share-tab');if(!b)return null;const r=b.getBoundingClientRect();return {text:b.textContent.trim(),disabled:b.disabled,visible:r.width>0&&r.height>0,x:r.left+r.width/2,y:r.top+r.height/2};})()"
    returnByValue = $true
  }
  $button = $buttonResult.result.value
  if (-not $button -or $button.disabled -or -not $button.visible) {
    throw "Share button is missing, disabled, or not visible."
  }

  Invoke-Cdp $mainSocket ([ref]$mainSequence) "Input.dispatchMouseEvent" @{
    type = "mousePressed"
    x = $button.x
    y = $button.y
    button = "left"
    clickCount = 1
  } | Out-Null
  Invoke-Cdp $mainSocket ([ref]$mainSequence) "Input.dispatchMouseEvent" @{
    type = "mouseReleased"
    x = $button.x
    y = $button.y
    button = "left"
    clickCount = 1
  } | Out-Null

  $shareTarget = $null
  for ($attempt = 0; $attempt -lt 30; $attempt += 1) {
    Start-Sleep -Milliseconds 250
    $shareTarget = Get-CdpTargets | Where-Object {
      $_.type -eq "page" -and $_.url -like "*share.html*"
    } | Select-Object -First 1
    if ($shareTarget) {
      break
    }
  }
  if (-not $shareTarget) {
    $statusResult = Invoke-Cdp $mainSocket ([ref]$mainSequence) "Runtime.evaluate" @{
      expression = "document.querySelector('#share-status')?.textContent"
      returnByValue = $true
    }
    throw "Share preview did not open. Status: $($statusResult.result.value)"
  }
} finally {
  $mainSocket.Dispose()
}

$shareSocket = Connect-CdpSocket $shareTarget.webSocketDebuggerUrl
$shareSequence = 0
$browserLaunchRequested = $false
$selectedBrowser = ""
$senderPageStatus = 0
$senderUrl = ""
try {
  Invoke-Cdp $shareSocket ([ref]$shareSequence) "Runtime.enable" | Out-Null
  $shareResult = Invoke-Cdp $shareSocket ([ref]$shareSequence) "Runtime.evaluate" @{
    expression = "({ready:document.readyState,title:document.title,heading:document.querySelector('h1')?.textContent.trim(),hasBrowserSelect:!!document.querySelector('#share-browser'),hasLaunchButton:!!document.querySelector('#launch-browser-share'),hasDirectShare:!!document.querySelector('#start-direct-share'),error:document.querySelector('#share-error')?.textContent||''})"
    returnByValue = $true
  }
  $share = $shareResult.result.value
  if ($share.ready -ne "complete" -or -not $share.hasBrowserSelect -or -not $share.hasLaunchButton) {
    throw "Share preview DOM is incomplete."
  }
  if ($share.hasDirectShare) {
    throw "The Tauri share preview must not expose direct display capture."
  }
  if ($LaunchBrowser) {
    $launchResult = Invoke-Cdp $shareSocket ([ref]$shareSequence) "Runtime.evaluate" @{
      expression = "(()=>{const b=document.querySelector('#launch-browser-share');const s=document.querySelector('#share-browser');if(!b||!s)return null;const r=b.getBoundingClientRect();return {disabled:b.disabled,visible:r.width>0&&r.height>0,x:r.left+r.width/2,y:r.top+r.height/2,browser:s.value};})()"
      returnByValue = $true
    }
    $launch = $launchResult.result.value
    if (-not $launch -or $launch.disabled -or -not $launch.visible) {
      throw "External browser launch button is unavailable."
    }
    $selectedBrowser = $launch.browser
    Invoke-Cdp $shareSocket ([ref]$shareSequence) "Input.dispatchMouseEvent" @{
      type = "mousePressed"
      x = $launch.x
      y = $launch.y
      button = "left"
      clickCount = 1
    } | Out-Null
    Invoke-Cdp $shareSocket ([ref]$shareSequence) "Input.dispatchMouseEvent" @{
      type = "mouseReleased"
      x = $launch.x
      y = $launch.y
      button = "left"
      clickCount = 1
    } | Out-Null

    for ($attempt = 0; $attempt -lt 24; $attempt += 1) {
      Start-Sleep -Milliseconds 250
      $launchStateResult = Invoke-Cdp $shareSocket ([ref]$shareSequence) "Runtime.evaluate" @{
        expression = "({waiting:!document.querySelector('#waiting-message')?.classList.contains('hidden'),error:document.querySelector('#share-error')?.textContent||''})"
        returnByValue = $true
      }
      $launchState = $launchStateResult.result.value
      if ($launchState.error) {
        throw "External browser launch failed: $($launchState.error)"
      }
      if ($launchState.waiting) {
        $browserLaunchRequested = $true
        break
      }
    }
    if (-not $browserLaunchRequested) {
      throw "External browser sharing page did not enter the waiting state."
    }
    $sessionEndpoint = ""
    for ($attempt = 0; $attempt -lt 24; $attempt += 1) {
      $endpointResult = Invoke-Cdp $shareSocket ([ref]$shareSequence) "Runtime.evaluate" @{
        expression = "performance.getEntriesByType('resource').map(entry=>entry.name).find(name=>name.includes('/api/session/'))||''"
        returnByValue = $true
      }
      $sessionEndpoint = $endpointResult.result.value
      if ($sessionEndpoint) {
        break
      }
      Start-Sleep -Milliseconds 250
    }
    if (-not $sessionEndpoint) {
      throw "Local sharing session endpoint was not observed."
    }
    $sessionUri = [Uri]$sessionEndpoint
    $token = $sessionUri.AbsolutePath.Split("/")[-1]
    $senderUrl = "$($sessionUri.Scheme)://$($sessionUri.Authority)/share/$token"
    $senderResponse = Invoke-WebRequest -UseBasicParsing -Uri $senderUrl -TimeoutSec 3
    if ($senderResponse.Content -notlike "*共有するタブを選択*") {
      throw "External browser sharing page content is incomplete."
    }
    $senderPageStatus = $senderResponse.StatusCode
  }
} finally {
  $shareSocket.Dispose()
}

[pscustomobject]@{
  MainTarget = $mainTarget.title
  ShareButton = $button.text
  EmbeddedBrowserRemoved = $layout.removed
  RecordingSharePanel = $layout.hasPanel
  ShareWindow = $share.title
  Heading = $share.heading
  BrowserSelector = $share.hasBrowserSelect
  LaunchButton = $share.hasLaunchButton
  DirectShareButton = $share.hasDirectShare
  SelectedBrowser = $selectedBrowser
  BrowserLaunchRequested = $browserLaunchRequested
  SenderPageStatus = $senderPageStatus
  SenderUrl = $senderUrl
  Error = $share.error
} | ConvertTo-Json -Depth 4
