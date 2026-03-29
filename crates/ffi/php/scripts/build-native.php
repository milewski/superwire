<?php

declare(strict_types=1);

$packageDirectory = \dirname(__DIR__);
$workspaceRootDirectory = resolveWorkspaceRootDirectory($packageDirectory);
$nativeDirectory = $packageDirectory . '/native';
$sourceLibraryPath = $workspaceRootDirectory . '/target/release/' . libraryFileNameForCurrentPlatform();
$destinationLibraryPath = $nativeDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;

runCommand(['cargo', 'build', '-p', 'ffi', '--release', '--features', 'php-ext'], $workspaceRootDirectory);

if (!\is_file($sourceLibraryPath)) {
    throw new RuntimeException("Rust PHP extension artifact was not produced at {$sourceLibraryPath}");
}

if (!\is_dir($nativeDirectory) && !\mkdir($nativeDirectory, 0o755, true) && !\is_dir($nativeDirectory)) {
    throw new RuntimeException("Unable to create native directory: {$nativeDirectory}");
}

if (!\copy($sourceLibraryPath, $destinationLibraryPath)) {
    throw new RuntimeException("Unable to copy native extension to {$destinationLibraryPath}");
}

print "Built native extension at {$destinationLibraryPath}\n";

function resolveWorkspaceRootDirectory(string $packageDirectory): string
{
    $workspaceRootOverride = \getenv('ENGINE_AI_FFI_PHP_WORKSPACE_ROOT');

    if (\is_string($workspaceRootOverride) && $workspaceRootOverride !== '') {
        $resolvedOverridePath = \realpath($workspaceRootOverride);

        if ($resolvedOverridePath === false) {
            throw new RuntimeException('ENGINE_AI_FFI_PHP_WORKSPACE_ROOT points to a path that does not exist.');
        }

        if (!\is_file($resolvedOverridePath . '/Cargo.toml')) {
            throw new RuntimeException('ENGINE_AI_FFI_PHP_WORKSPACE_ROOT does not look like a Cargo workspace root.');
        }

        return $resolvedOverridePath;
    }

    $detectedWorkspaceRoot = \dirname($packageDirectory, 3);

    if (!\is_file($detectedWorkspaceRoot . '/Cargo.toml')) {
        throw new RuntimeException(
            'Unable to locate Cargo workspace root automatically. Set ENGINE_AI_FFI_PHP_WORKSPACE_ROOT to build from source.',
        );
    }

    return $detectedWorkspaceRoot;
}

function libraryFileNameForCurrentPlatform(): string
{
    return match (PHP_OS_FAMILY) {
        'Darwin' => 'libffi.dylib',
        'Linux' => 'libffi.so',
        'Windows' => 'ffi.dll',
        default => throw new RuntimeException('Unsupported platform for engine-ai ffi PHP extension build.'),
    };
}

/**
 * @param array<int, string> $commandParts
 */
function runCommand(array $commandParts, string $workingDirectory): void
{
    $descriptors = [
        0 => ['pipe', 'r'],
        1 => ['pipe', 'w'],
        2 => ['pipe', 'w'],
    ];

    $process = \proc_open($commandParts, $descriptors, $pipes, $workingDirectory);

    if (!\is_resource($process)) {
        throw new RuntimeException('Unable to start build command process.');
    }

    \fclose($pipes[0]);
    $standardOutput = \stream_get_contents($pipes[1]);
    $standardError = \stream_get_contents($pipes[2]);
    \fclose($pipes[1]);
    \fclose($pipes[2]);

    $exitCode = \proc_close($process);

    if ($standardOutput !== false && $standardOutput !== '') {
        print $standardOutput;
    }

    if ($standardError !== false && $standardError !== '') {
        fwrite(STDERR, $standardError);
    }

    if ($exitCode !== 0) {
        throw new RuntimeException('Native extension build command failed.');
    }
}
