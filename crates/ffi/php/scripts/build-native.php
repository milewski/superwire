<?php

declare(strict_types = 1);

$packageDirectory = \dirname(__DIR__);
$workspaceRootDirectory = resolveWorkspaceRootDirectory($packageDirectory);
$nativeDirectory = $packageDirectory . '/native';
$sourceLibraryPath = $workspaceRootDirectory . '/target/release/' . libraryFileNameForCurrentPlatform();
$destinationLibraryPath = $nativeDirectory . '/engine_ai_ffi.' . PHP_SHLIB_SUFFIX;
$phpConfigPath = resolvePhpConfigPath();

runCommand(
    [ 'cargo', 'build', '-p', 'ffi', '--release', '--features', 'php-ext' ],
    $workspaceRootDirectory,
    [
        'PHP_CONFIG' => $phpConfigPath,
    ],
);

if (!\is_file($sourceLibraryPath)) {
    throw new RuntimeException("Rust PHP extension artifact was not produced at {$sourceLibraryPath}");
}

if (!\is_dir($nativeDirectory) && !\mkdir($nativeDirectory, 0o755, true) && !\is_dir($nativeDirectory)) {
    throw new RuntimeException("Unable to create native directory: {$nativeDirectory}");
}

if (!\copy($sourceLibraryPath, $destinationLibraryPath)) {
    throw new RuntimeException("Unable to copy native extension to {$destinationLibraryPath}");
}

echo "Built native extension at {$destinationLibraryPath}\n";

function resolvePhpConfigPath(): string
{
    $configuredPhpConfigPath = \getenv('PHP_CONFIG');

    if (\is_string($configuredPhpConfigPath) && $configuredPhpConfigPath !== '') {

        if (!isExecutableFile($configuredPhpConfigPath)) {
            throw new RuntimeException('PHP_CONFIG is set but does not point to an executable file.');
        }

        return $configuredPhpConfigPath;

    }

    $phpMajorVersion = PHP_MAJOR_VERSION;
    $phpMinorVersion = PHP_MINOR_VERSION;

    $candidatePhpConfigPaths = [
        findBinaryInPath('php-config'),
        findBinaryInPath("php-config{$phpMajorVersion}.{$phpMinorVersion}"),
        findBinaryInPath("php-config{$phpMajorVersion}{$phpMinorVersion}"),
        findBinaryInPath("php{$phpMajorVersion}.{$phpMinorVersion}-config"),
        findBinaryInPath("php{$phpMajorVersion}{$phpMinorVersion}-config"),
    ];

    foreach ($candidatePhpConfigPaths as $candidatePhpConfigPath) {

        if ($candidatePhpConfigPath === null) {
            continue;
        }

        return $candidatePhpConfigPath;

    }

    throw new RuntimeException(
        'Could not find `php-config`. Install PHP development headers (for Ubuntu/Debian: `sudo apt install php-dev` '
            . 'or `sudo apt install php8.3-dev`) or set PHP_CONFIG=/path/to/php-config.',
    );
}

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

function findBinaryInPath(string $binaryName): ?string
{
    $descriptorSpecification = [
        0 => [ 'pipe', 'r' ],
        1 => [ 'pipe', 'w' ],
        2 => [ 'pipe', 'w' ],
    ];

    $process = \proc_open([ 'sh', '-lc', "command -v {$binaryName}" ], $descriptorSpecification, $pipes);

    if (!\is_resource($process)) {
        return null;
    }

    \fclose($pipes[ 0 ]);
    $standardOutput = \stream_get_contents($pipes[ 1 ]);
    \stream_get_contents($pipes[ 2 ]);
    \fclose($pipes[ 1 ]);
    \fclose($pipes[ 2 ]);

    $exitCode = \proc_close($process);

    if ($exitCode !== 0 || $standardOutput === false) {
        return null;
    }

    $resolvedPath = \trim($standardOutput);

    if ($resolvedPath === '' || !isExecutableFile($resolvedPath)) {
        return null;
    }

    return $resolvedPath;
}

function isExecutableFile(string $filePath): bool
{
    return \is_file($filePath) && \is_executable($filePath);
}

/**
 * @param array<int, string> $commandParts
 * @param array<string, string> $additionalEnvironment
 */
function runCommand(array $commandParts, string $workingDirectory, array $additionalEnvironment = []): void
{
    $descriptors = [
        0 => [ 'pipe', 'r' ],
        1 => [ 'pipe', 'w' ],
        2 => [ 'pipe', 'w' ],
    ];

    $baseEnvironment = \getenv();

    if (!\is_array($baseEnvironment)) {
        $baseEnvironment = [];
    }

    $commandEnvironment = [ ...$baseEnvironment, ...$additionalEnvironment ];
    $process = \proc_open($commandParts, $descriptors, $pipes, $workingDirectory, $commandEnvironment);

    if (!\is_resource($process)) {
        throw new RuntimeException('Unable to start build command process.');
    }

    \fclose($pipes[ 0 ]);
    $standardOutput = \stream_get_contents($pipes[ 1 ]);
    $standardError = \stream_get_contents($pipes[ 2 ]);
    \fclose($pipes[ 1 ]);
    \fclose($pipes[ 2 ]);

    $exitCode = \proc_close($process);

    if ($standardOutput !== false && $standardOutput !== '') {
        echo $standardOutput;
    }

    if ($standardError !== false && $standardError !== '') {
        fwrite(STDERR, $standardError);
    }

    if ($exitCode !== 0) {
        throw new RuntimeException('Native extension build command failed.');
    }
}
