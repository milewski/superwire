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
    $resolvedSourceBinaryPath = '';

    if (\is_file($prebuiltBinaryPath)) {
        $resolvedSourceBinaryPath = $prebuiltBinaryPath;

        print "Found prebuilt native extension for " . platformKey() . "\n";
    }

    if ($resolvedSourceBinaryPath === '') {
        if (!\is_file($localBuiltBinaryPath)) {
            print "No prebuilt binary found for " . platformKey() . ". Building extension from source...\n";
            require __DIR__ . '/build-native.php';
        }

        $resolvedSourceBinaryPath = $localBuiltBinaryPath;
    }

    $resolvedExtensionDirectory = resolvePhpExtensionDirectory();
    $installedBinaryPath = installBinary($resolvedSourceBinaryPath, $resolvedExtensionDirectory);
    $resolvedIniFilePath = installIniFile($installedBinaryPath);

    print "Native extension binary ready at {$installedBinaryPath}\n";

    if ($resolvedIniFilePath !== null) {
        print "Extension ini written at {$resolvedIniFilePath}\n";

        return;
    }

    if (\getenv('ENGINE_AI_FFI_PHP_INI_DIR') === false) {
        print "Set ENGINE_AI_FFI_PHP_INI_DIR to auto-write an ini file inside a writable directory.\n";
    }

    print "Enable it in php.ini with: extension={$installedBinaryPath}\n";
}

function resolvePhpExtensionDirectory(): string
{
    $overrideExtensionDirectory = \getenv('ENGINE_AI_FFI_PHP_EXTENSION_DIR');

    if (\is_string($overrideExtensionDirectory) && $overrideExtensionDirectory !== '') {
        if (!\is_dir($overrideExtensionDirectory)) {
            if (!@\mkdir($overrideExtensionDirectory, 0o755, true) && !\is_dir($overrideExtensionDirectory)) {
                throw new RuntimeException("ENGINE_AI_FFI_PHP_EXTENSION_DIR does not exist: {$overrideExtensionDirectory}");
            }
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

function installBinary(string $sourcePath, string $extensionDirectory): string
{
    if (!\is_file($sourcePath)) {
        throw new RuntimeException("Native extension binary does not exist: {$sourcePath}");
    }

    $targetPath = $extensionDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

    if (!\is_writable($extensionDirectory)) {
        print "PHP extension directory is not writable ({$extensionDirectory}); using package-local binary path.\n";

        return $sourcePath;
    }

    if (!@\copy($sourcePath, $targetPath)) {
        print "Unable to copy native extension binary to {$targetPath}; using package-local binary path.\n";

        return $sourcePath;
    }

    return $targetPath;
}

function installIniFile(string $installedBinaryPath): ?string
{
    $iniDirectory = resolveIniDirectory();

    if ($iniDirectory === null) {
        return null;
    }

    if (!\is_dir($iniDirectory)) {
        if (!@\mkdir($iniDirectory, 0o755, true) && !\is_dir($iniDirectory)) {
            throw new RuntimeException("ENGINE_AI_FFI_PHP_INI_DIR is not writable: {$iniDirectory}");
        }
    }

    if (!\is_writable($iniDirectory)) {
        throw new RuntimeException("ENGINE_AI_FFI_PHP_INI_DIR is not writable: {$iniDirectory}");
    }

    $iniFileName = \getenv('ENGINE_AI_FFI_PHP_INI_FILENAME');

    if (!\is_string($iniFileName) || $iniFileName === '') {
        $iniFileName = '99-engine-ai-ffi.ini';
    }

    $iniFilePath = \rtrim($iniDirectory, '/\\') . '/' . $iniFileName;
    $iniContents = "extension={$installedBinaryPath}\n";

    if (\file_put_contents($iniFilePath, $iniContents) === false) {
        throw new RuntimeException("Unable to write ini file: {$iniFilePath}");
    }

    return $iniFilePath;
}

function resolveIniDirectory(): ?string
{
    $overrideIniDirectory = \getenv('ENGINE_AI_FFI_PHP_INI_DIR');

    if (\is_string($overrideIniDirectory) && $overrideIniDirectory !== '') {
        return $overrideIniDirectory;
    }

    $phpIniScanDirectory = \getenv('PHP_INI_SCAN_DIR');

    if (!\is_string($phpIniScanDirectory) || $phpIniScanDirectory === '') {
        return null;
    }

    $scanDirectories = \explode(PATH_SEPARATOR, $phpIniScanDirectory);

    foreach ($scanDirectories as $scanDirectory) {
        $normalizedDirectory = \trim($scanDirectory);

        if ($normalizedDirectory === '') {
            continue;
        }

        if (\is_dir($normalizedDirectory)) {
            if (\is_writable($normalizedDirectory)) {
                return $normalizedDirectory;
            }

            continue;
        }

        $parentDirectory = \dirname($normalizedDirectory);

        if (!\is_dir($parentDirectory) || !\is_writable($parentDirectory)) {
            continue;
        }

        return $normalizedDirectory;
    }

    return null;
}
