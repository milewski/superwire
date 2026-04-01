<?php

declare(strict_types=1);

$packageDirectory = \dirname(__DIR__);
$nativeDirectory = $packageDirectory . '/native';
$sourceBinaryPath = $nativeDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
$targetDirectory = $nativeDirectory . '/prebuilt/' . runtimePlatformKey();
$targetBinaryPath = $targetDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

if (!\is_file($sourceBinaryPath)) {
    require __DIR__ . '/build-native.php';
}

if (!\is_dir($targetDirectory) && !\mkdir($targetDirectory, 0o755, true) && !\is_dir($targetDirectory)) {
    throw new RuntimeException("Unable to create prebuilt target directory: {$targetDirectory}");
}

if (!\copy($sourceBinaryPath, $targetBinaryPath)) {
    throw new RuntimeException("Unable to package prebuilt binary at {$targetBinaryPath}");
}

$checksumPath = $targetBinaryPath . '.sha256';
$checksum = \hash_file('sha256', $targetBinaryPath);

if (!\is_string($checksum) || $checksum === '') {
    throw new RuntimeException("Unable to compute checksum for {$targetBinaryPath}");
}

$checksumContents = $checksum . '  ' . \basename($targetBinaryPath) . "\n";

if (\file_put_contents($checksumPath, $checksumContents) === false) {
    throw new RuntimeException("Unable to write checksum file at {$checksumPath}");
}

print "Packaged prebuilt binary at {$targetBinaryPath}\n";
print "Wrote checksum at {$checksumPath}\n";

function runtimePlatformKey(): string
{
    return platformKey() . '-' . phpRuntimeKey();
}

function phpRuntimeKey(): string
{
    $phpVersionKey = 'php' . PHP_MAJOR_VERSION . '.' . PHP_MINOR_VERSION;
    $threadingKey = PHP_ZTS === 1 ? 'zts' : 'nts';

    return "{$phpVersionKey}-{$threadingKey}";
}

function platformKey(): string
{
    $normalizedOperatingSystem = \strtolower(PHP_OS_FAMILY);
    $normalizedArchitecture = normalizeArchitecture(\php_uname('m'));
    $libcVariant = normalizeLibcVariant();

    if ($normalizedOperatingSystem !== 'linux') {
        return "{$normalizedOperatingSystem}-{$normalizedArchitecture}";
    }

    return "{$normalizedOperatingSystem}-{$normalizedArchitecture}-{$libcVariant}";
}

function normalizeArchitecture(string $architecture): string
{
    $normalizedArchitecture = \strtolower($architecture);

    return match ($normalizedArchitecture) {
        'x64', 'amd64' => 'x86_64',
        'arm64' => 'aarch64',
        default => $normalizedArchitecture,
    };
}

function normalizeLibcVariant(): string
{
    $detectedLibcVersionString = \function_exists('phpversion') ? \phpversion('libc') : false;

    if (\is_string($detectedLibcVersionString) && $detectedLibcVersionString !== '') {
        $normalizedLibcVersionString = \strtolower($detectedLibcVersionString);

        if (\str_contains($normalizedLibcVersionString, 'musl')) {
            return 'musl';
        }

        if (\str_contains($normalizedLibcVersionString, 'gnu') || \str_contains($normalizedLibcVersionString, 'glibc')) {
            return 'gnu';
        }
    }

    $libcVersionCommandOutput = trim((string) @\shell_exec('ldd --version 2>&1'));
    $normalizedCommandOutput = \strtolower($libcVersionCommandOutput);

    if (\str_contains($normalizedCommandOutput, 'musl')) {
        return 'musl';
    }

    return 'gnu';
}
