<?php

declare(strict_types = 1);

$packageDirectory = \dirname(__DIR__);
$phpFiles = [];

collectPhpFiles($packageDirectory . '/src', $phpFiles);
collectPhpFiles($packageDirectory . '/examples', $phpFiles);
collectPhpFiles($packageDirectory . '/scripts', $phpFiles);

foreach ($phpFiles as $phpFilePath) {
    lintFile($phpFilePath);
}

echo 'Lint completed for ' . \count($phpFiles) . " files.\n";

/**
 * @param array<int, string> $phpFiles
 */
function collectPhpFiles(string $directory, array &$phpFiles): void
{
    if (!\is_dir($directory)) {
        return;
    }

    $directoryIterator = new RecursiveDirectoryIterator($directory);
    $iterator = new RecursiveIteratorIterator($directoryIterator);

    foreach ($iterator as $fileInfo) {

        if (!$fileInfo instanceof SplFileInfo) {
            continue;
        }

        if (!$fileInfo->isFile()) {
            continue;
        }

        if ($fileInfo->getExtension() !== 'php') {
            continue;
        }

        $phpFiles[] = $fileInfo->getPathname();

    }
}

function lintFile(string $phpFilePath): void
{
    $commandParts = [ 'php', '-l', $phpFilePath ];
    $descriptorSpecification = [
        0 => [ 'pipe', 'r' ],
        1 => [ 'pipe', 'w' ],
        2 => [ 'pipe', 'w' ],
    ];

    $process = \proc_open($commandParts, $descriptorSpecification, $pipes);

    if (!\is_resource($process)) {
        throw new RuntimeException("Unable to lint file: {$phpFilePath}");
    }

    \fclose($pipes[ 0 ]);
    $standardOutput = \stream_get_contents($pipes[ 1 ]);
    $standardError = \stream_get_contents($pipes[ 2 ]);
    \fclose($pipes[ 1 ]);
    \fclose($pipes[ 2 ]);

    $exitCode = \proc_close($process);

    if ($exitCode !== 0) {

        if ($standardOutput !== false && $standardOutput !== '') {
            echo $standardOutput;
        }

        if ($standardError !== false && $standardError !== '') {
            fwrite(STDERR, $standardError);
        }

        throw new RuntimeException("PHP lint failed for {$phpFilePath}");

    }
}
