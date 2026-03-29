<?php

declare(strict_types=1);

try {
    installNativeExtension();
} catch (\Throwable $throwable) {
    fwrite(STDERR, "Native extension install skipped: {$throwable->getMessage()}\n");
    fwrite(STDERR, "Run `composer build-native` then enable extension=engine_ai_ffi manually.\n");
}

function installNativeExtension(): void
{
    if ((string) \getenv('ENGINE_AI_FFI_PHP_SKIP_NATIVE_INSTALL') === '1') {
        print "Skipping native install because ENGINE_AI_FFI_PHP_SKIP_NATIVE_INSTALL=1\n";

        return;
    }

    $packageDirectory = \dirname(__DIR__);
    $nativeDirectory = $packageDirectory . '/native';
    $localBuiltBinaryPath = $nativeDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
    $prebuiltBinaryPath = $nativeDirectory . '/prebuilt/' . platformKey() . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

    if (\is_file($prebuiltBinaryPath)) {
        installBinary($prebuiltBinaryPath, resolvePhpExtensionDirectory());
        print "Installed prebuilt native extension from {$prebuiltBinaryPath}\n";

        return;
    }

    if (!\is_file($localBuiltBinaryPath)) {
        print "No prebuilt binary found for " . platformKey() . ". Building extension from source...\n";
        require __DIR__ . '/build-native.php';
    }

    installBinary($localBuiltBinaryPath, resolvePhpExtensionDirectory());

    print "Installed native extension from {$localBuiltBinaryPath}\n";
    print "Enable it in php.ini with: extension=engine_ai_ffi\n";
}

function resolvePhpExtensionDirectory(): string
{
    $overrideExtensionDirectory = \getenv('ENGINE_AI_FFI_PHP_EXTENSION_DIR');

    if (\is_string($overrideExtensionDirectory) && $overrideExtensionDirectory !== '') {
        if (!\is_dir($overrideExtensionDirectory)) {
            throw new RuntimeException("ENGINE_AI_FFI_PHP_EXTENSION_DIR does not exist: {$overrideExtensionDirectory}");
        }

        return $overrideExtensionDirectory;
    }

    $extensionDirectory = \ini_get('extension_dir');

    if (!\is_string($extensionDirectory) || $extensionDirectory === '') {
        throw new RuntimeException('Unable to resolve PHP extension_dir from ini settings.');
    }

    if (!\is_dir($extensionDirectory)) {
        throw new RuntimeException("PHP extension_dir does not exist: {$extensionDirectory}");
    }

    return $extensionDirectory;
}

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

function installBinary(string $sourcePath, string $extensionDirectory): void
{
    if (!\is_file($sourcePath)) {
        throw new RuntimeException("Native extension binary does not exist: {$sourcePath}");
    }

    $targetPath = $extensionDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

    if (!\is_writable($extensionDirectory)) {
        throw new RuntimeException(
            "The PHP extension directory is not writable: {$extensionDirectory}. " .
                "Copy manually with sudo: sudo cp {$sourcePath} {$targetPath}",
        );
    }

    if (!@\copy($sourcePath, $targetPath)) {
        throw new RuntimeException(
            "Unable to copy native extension binary to {$targetPath}. " .
                "Try: sudo cp {$sourcePath} {$targetPath}",
        );
    }
}
