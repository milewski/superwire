<?php

declare(strict_types=1);

const ENGINE_AI_FFI_PHP_INSTALL_MODE_LOCAL = 'local';
const ENGINE_AI_FFI_PHP_INSTALL_MODE_SYSTEM = 'system';

try {
    installNativeExtension();
} catch (\Throwable $throwable) {
    fwrite(STDERR, "Native extension install skipped: {$throwable->getMessage()}\n");
    fwrite(
        STDERR,
        "Run `composer install-native` once prebuilt artifacts are available, or set ENGINE_AI_FFI_PHP_BUILD_FROM_SOURCE=1 and run `composer install-native` to compile from source.\n",
    );
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
    $prebuiltPlatformKey = runtimePlatformKey();
    $prebuiltBinaryPath = $nativeDirectory . '/prebuilt/' . $prebuiltPlatformKey . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
    $legacyRuntimePrebuiltBinaryPath = $nativeDirectory . '/prebuilt/' . platformKey() . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
    $legacyPrebuiltBinaryPath = $nativeDirectory . '/prebuilt/' . legacyPlatformKey() . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
    $resolvedSourceBinaryPath = '';

    if (\is_file($prebuiltBinaryPath)) {
        $resolvedSourceBinaryPath = $prebuiltBinaryPath;

        print "Found prebuilt native extension for {$prebuiltPlatformKey}\n";
    }

    if ($resolvedSourceBinaryPath === '' && \is_file($legacyRuntimePrebuiltBinaryPath)) {
        $resolvedSourceBinaryPath = $legacyRuntimePrebuiltBinaryPath;

        print "Found legacy prebuilt native extension for " . platformKey() . "\n";
    }

    if ($resolvedSourceBinaryPath === '' && \is_file($legacyPrebuiltBinaryPath)) {
        $resolvedSourceBinaryPath = $legacyPrebuiltBinaryPath;

        print "Found legacy prebuilt native extension for " . legacyPlatformKey() . "\n";
    }

    if ($resolvedSourceBinaryPath === '') {
        $downloadedPrebuiltBinaryPath = downloadRemotePrebuiltBinary($nativeDirectory, $prebuiltPlatformKey);

        if ($downloadedPrebuiltBinaryPath !== null) {
            $resolvedSourceBinaryPath = $downloadedPrebuiltBinaryPath;

            print "Downloaded prebuilt native extension for {$prebuiltPlatformKey}\n";
        }
    }

    if ($resolvedSourceBinaryPath === '') {
        if (\is_file($localBuiltBinaryPath)) {
            $resolvedSourceBinaryPath = $localBuiltBinaryPath;
            print "Using locally built native extension binary at {$localBuiltBinaryPath}\n";
        }
    }

    if ($resolvedSourceBinaryPath === '') {
        if (!shouldBuildFromSource()) {
            throw new RuntimeException(
                "No prebuilt binary available for {$prebuiltPlatformKey}. " .
                'Set ENGINE_AI_FFI_PHP_BUILD_FROM_SOURCE=1 to compile from source.',
            );
        }

        print "No prebuilt binary found for {$prebuiltPlatformKey}. Building extension from source...\n";
        require __DIR__ . '/build-native.php';
        $resolvedSourceBinaryPath = $localBuiltBinaryPath;
    }

    $installedBinaryPath = installBinary($resolvedSourceBinaryPath);
    validateBinaryCanLoad($installedBinaryPath);
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

function shouldBuildFromSource(): bool
{
    $rawValue = \getenv('ENGINE_AI_FFI_PHP_BUILD_FROM_SOURCE');

    if (!\is_string($rawValue)) {
        return false;
    }

    return \in_array(\strtolower(\trim($rawValue)), ['1', 'true', 'yes', 'on'], true);
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

function resolveInstallMode(): string
{
    $rawInstallMode = \getenv('ENGINE_AI_FFI_PHP_INSTALL_MODE');

    if (!\is_string($rawInstallMode) || $rawInstallMode === '') {
        return ENGINE_AI_FFI_PHP_INSTALL_MODE_LOCAL;
    }

    $normalizedInstallMode = \strtolower(\trim($rawInstallMode));

    if ($normalizedInstallMode !== ENGINE_AI_FFI_PHP_INSTALL_MODE_LOCAL
        && $normalizedInstallMode !== ENGINE_AI_FFI_PHP_INSTALL_MODE_SYSTEM) {
        print "Unknown ENGINE_AI_FFI_PHP_INSTALL_MODE={$rawInstallMode}. Falling back to local mode.\n";

        return ENGINE_AI_FFI_PHP_INSTALL_MODE_LOCAL;
    }

    return $normalizedInstallMode;
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

function legacyPlatformKey(): string
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

function installBinary(string $sourcePath): string
{
    if (!\is_file($sourcePath)) {
        throw new RuntimeException("Native extension binary does not exist: {$sourcePath}");
    }

    $overrideExtensionDirectory = \getenv('ENGINE_AI_FFI_PHP_EXTENSION_DIR');

    if (\is_string($overrideExtensionDirectory) && $overrideExtensionDirectory !== '') {
        return copyBinaryToDirectory($sourcePath, $overrideExtensionDirectory);
    }

    $installMode = resolveInstallMode();

    if ($installMode === ENGINE_AI_FFI_PHP_INSTALL_MODE_LOCAL) {
        print "Using package-local extension binary path. Set ENGINE_AI_FFI_PHP_INSTALL_MODE=system to copy into extension_dir.\n";

        return $sourcePath;
    }

    $extensionDirectory = resolvePhpExtensionDirectory();

    return copyBinaryToDirectory($sourcePath, $extensionDirectory);
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

function copyBinaryToDirectory(string $sourcePath, string $targetDirectory): string
{
    if (!\is_dir($targetDirectory)) {
        if (!@\mkdir($targetDirectory, 0o755, true) && !\is_dir($targetDirectory)) {
            throw new RuntimeException("Extension directory does not exist: {$targetDirectory}");
        }
    }

    $targetPath = $targetDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

    if (!\is_writable($targetDirectory)) {
        print "Extension directory is not writable ({$targetDirectory}); using package-local binary path.\n";

        return $sourcePath;
    }

    if (!@\copy($sourcePath, $targetPath)) {
        print "Unable to copy native extension binary to {$targetPath}; using package-local binary path.\n";

        return $sourcePath;
    }

    return $targetPath;
}

function downloadRemotePrebuiltBinary(string $nativeDirectory, string $prebuiltPlatformKey): ?string
{
    if ((string) \getenv('ENGINE_AI_FFI_PHP_SKIP_REMOTE_PREBUILT') === '1') {
        print "Skipping remote prebuilt download because ENGINE_AI_FFI_PHP_SKIP_REMOTE_PREBUILT=1\n";

        return null;
    }

    $downloadDirectory = $nativeDirectory . '/downloaded/' . $prebuiltPlatformKey;

    if (!\is_dir($downloadDirectory) && !@\mkdir($downloadDirectory, 0o755, true) && !\is_dir($downloadDirectory)) {
        throw new RuntimeException("Unable to create downloaded prebuilt directory: {$downloadDirectory}");
    }

    $assetFileName = "engine_ai_ffi-{$prebuiltPlatformKey}." . PHP_SHLIB_SUFFIX;
    $checksumFileName = $assetFileName . '.sha256';
    $binaryDownloadUrl = prebuiltAssetUrl($assetFileName);
    $checksumDownloadUrl = prebuiltAssetUrl($checksumFileName);
    $binaryPath = $downloadDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
    $checksumPath = $downloadDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX . '.sha256';

    if (!downloadFile($binaryDownloadUrl, $binaryPath)) {
        print "No remote prebuilt binary found for {$prebuiltPlatformKey} at {$binaryDownloadUrl}\n";

        return null;
    }

    if (!downloadFile($checksumDownloadUrl, $checksumPath)) {
        throw new RuntimeException(
            "Downloaded prebuilt binary without checksum: {$checksumDownloadUrl}",
        );
    }

    verifySha256Checksum($binaryPath, $checksumPath);

    return $binaryPath;
}

function prebuiltAssetUrl(string $fileName): string
{
    $baseUrl = \getenv('ENGINE_AI_FFI_PHP_PREBUILT_BASE_URL');

    if (!\is_string($baseUrl) || $baseUrl === '') {
        $baseUrl = 'https://github.com/engine-ai/engine-ai/releases';
    }

    $baseUrl = \rtrim($baseUrl, '/');
    $tag = \getenv('ENGINE_AI_FFI_PHP_PREBUILT_TAG');

    if (!\is_string($tag) || $tag === '') {
        $tag = 'latest';
    }

    if ($tag === 'latest') {
        return "{$baseUrl}/latest/download/{$fileName}";
    }

    return "{$baseUrl}/download/{$tag}/{$fileName}";
}

function downloadFile(string $url, string $destinationPath): bool
{
    $context = \stream_context_create([
        'http' => [
            'method' => 'GET',
            'follow_location' => 1,
            'timeout' => 30,
        ],
    ]);

    $contents = @\file_get_contents($url, false, $context);

    if (!\is_string($contents) || $contents === '') {
        return false;
    }

    if (\file_put_contents($destinationPath, $contents) === false) {
        throw new RuntimeException("Unable to write downloaded file: {$destinationPath}");
    }

    return true;
}

function verifySha256Checksum(string $binaryPath, string $checksumPath): void
{
    $checksumContents = \trim((string) \file_get_contents($checksumPath));

    if ($checksumContents === '') {
        throw new RuntimeException("Checksum file is empty: {$checksumPath}");
    }

    $expectedChecksum = \preg_split('/\s+/', $checksumContents)[0] ?? '';

    if ($expectedChecksum === '') {
        throw new RuntimeException("Could not parse checksum from {$checksumPath}");
    }

    $actualChecksum = \hash_file('sha256', $binaryPath);

    if (!\is_string($actualChecksum) || $actualChecksum === '') {
        throw new RuntimeException("Could not compute checksum for {$binaryPath}");
    }

    if (!\hash_equals(\strtolower($expectedChecksum), \strtolower($actualChecksum))) {
        throw new RuntimeException(
            "Checksum verification failed for {$binaryPath}. Expected {$expectedChecksum}, got {$actualChecksum}.",
        );
    }
}

function validateBinaryCanLoad(string $binaryPath): void
{
    $descriptorSpecification = [
        0 => ['pipe', 'r'],
        1 => ['pipe', 'w'],
        2 => ['pipe', 'w'],
    ];

    $process = \proc_open([PHP_BINARY, '-n', '-d', "extension={$binaryPath}", '-m'], $descriptorSpecification, $pipes);

    if (!\is_resource($process)) {
        throw new RuntimeException('Unable to validate native extension loading.');
    }

    \fclose($pipes[0]);
    $standardOutput = \stream_get_contents($pipes[1]);
    $standardError = \stream_get_contents($pipes[2]);
    \fclose($pipes[1]);
    \fclose($pipes[2]);

    $exitCode = \proc_close($process);

    if ($exitCode === 0) {
        return;
    }

    $errorMessageParts = [
        "Native extension binary failed to load: {$binaryPath}",
        'This usually means ABI mismatch (for example glibc binary on Alpine/musl) or missing shared libraries.',
    ];

    if ($standardError !== false && \trim($standardError) !== '') {
        $errorMessageParts[] = 'Loader error: ' . \trim($standardError);
    }

    if ($standardOutput !== false && \trim($standardOutput) !== '') {
        $errorMessageParts[] = 'Loader output: ' . \trim($standardOutput);
    }

    throw new RuntimeException(\implode(' ', $errorMessageParts));
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
