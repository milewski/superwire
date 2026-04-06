param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release"
)

$scriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$repositoryRoot = Resolve-Path (Join-Path $scriptDirectory "../../..")
$extensionRoot = Resolve-Path (Join-Path $scriptDirectory "..")

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

if ($architecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
    $targetTriple = "aarch64-pc-windows-msvc"
    $bundleDirectory = "windows-aarch64"
} elseif ($architecture -eq [System.Runtime.InteropServices.Architecture]::X64) {
    $targetTriple = "x86_64-pc-windows-msvc"
    $bundleDirectory = "windows-x86_64"
} else {
    throw "Unsupported Windows architecture: $architecture"
}

$manifestPath = Join-Path $repositoryRoot "crates/lsp/Cargo.toml"
$cargoArguments = @("build", "--manifest-path", $manifestPath, "--bin", "superwire-lsp", "--target", $targetTriple)

if ($Profile -eq "release") {
    $cargoArguments += "--release"
}

Write-Host "Building superwire-lsp for $targetTriple ($Profile)..."
cargo @cargoArguments

if ($LASTEXITCODE -ne 0) {
    throw "Failed to build superwire-lsp"
}

$profileDirectory = if ($Profile -eq "release") { "release" } else { "debug" }
$compiledBinary = Join-Path $repositoryRoot "target/$targetTriple/$profileDirectory/superwire-lsp.exe"

if (-not (Test-Path $compiledBinary)) {
    throw "Compiled binary not found at $compiledBinary"
}

$bundleOutputDirectory = Join-Path $extensionRoot "bin/$bundleDirectory"
New-Item -ItemType Directory -Force -Path $bundleOutputDirectory | Out-Null

$bundleOutputBinary = Join-Path $bundleOutputDirectory "superwire-lsp.exe"
Copy-Item -Force $compiledBinary $bundleOutputBinary

Write-Host "Bundled binary written to $bundleOutputBinary"
