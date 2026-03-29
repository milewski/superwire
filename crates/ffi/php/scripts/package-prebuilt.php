<?php

declare(strict_types=1);

$packageDirectory = \dirname(__DIR__);
$nativeDirectory = $packageDirectory . '/native';
$sourceBinaryPath = $nativeDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
$targetDirectory = $nativeDirectory . '/prebuilt/' . platformKey();
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

print "Packaged prebuilt binary at {$targetBinaryPath}\n";

function platformKey(): string
{
    $normalizedOperatingSystem = \strtolower(PHP_OS_FAMILY);
    $normalizedArchitecture = normalizeArchitecture(\php_uname('m'));

    return "{$normalizedOperatingSystem}-{$normalizedArchitecture}";
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
