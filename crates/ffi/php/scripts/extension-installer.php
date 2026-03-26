<?php

declare(strict_types=1);

final class EngineAiFfiInstaller
{
    private string $crateManifestPath;

    public function __construct()
    {
        $this->crateManifestPath = dirname(__DIR__, 2) . '/Cargo.toml';
    }

    public function run(string $mode): int
    {
        if (!$this->isSupportedMode($mode)) {
            $this->writeToError('Unsupported mode. Use `build` or `install`.');

            return 1;
        }

        if (!$this->validatePrerequisites()) {
            return 1;
        }

        $cargoPhpSubcommand = $mode === 'install' ? 'install' : 'build';
        $command = sprintf(
            'cargo php %s --release --manifest-path %s',
            $cargoPhpSubcommand,
            escapeshellarg($this->crateManifestPath)
        );

        $exitCode = $this->runCommand($command);

        if ($exitCode !== 0) {
            $this->writeToError('Extension build/install failed.');

            return $exitCode;
        }

        if ($mode === 'install') {
            $this->writeToOutput('Extension installation completed.');
            $this->writeToOutput('Verify with: php --ri engine_ai_ffi');
        } else {
            $this->writeToOutput('Extension build completed.');
        }

        return 0;
    }

    private function isSupportedMode(string $mode): bool
    {
        return $mode === 'build' || $mode === 'install';
    }

    private function validatePrerequisites(): bool
    {
        if (PHP_VERSION_ID < 80200) {
            $this->writeToError('PHP 8.2+ is required.');

            return false;
        }

        if (!is_file($this->crateManifestPath)) {
            $this->writeToError('Unable to locate crates/ffi/Cargo.toml.');

            return false;
        }

        if (!$this->commandExists('cargo')) {
            $this->writeToError('Rust toolchain is required. Install cargo and rustc first.');

            return false;
        }

        if (!$this->hasCargoPhp()) {
            $this->writeToError('cargo-php is required. Install it with: cargo install cargo-php');

            return false;
        }

        return true;
    }

    private function commandExists(string $commandName): bool
    {
        $command = sprintf('command -v %s >/dev/null 2>&1', escapeshellarg($commandName));

        return $this->runCommand($command, false) === 0;
    }

    private function hasCargoPhp(): bool
    {
        return $this->runCommand('cargo php --version >/dev/null 2>&1', false) === 0;
    }

    private function runCommand(string $command, bool $outputEnabled = true): int
    {
        if ($outputEnabled) {
            passthru($command, $exitCode);

            return $exitCode;
        }

        exec($command, $ignoredOutput, $exitCode);

        return $exitCode;
    }

    private function writeToOutput(string $message): void
    {
        fwrite(STDOUT, $message . PHP_EOL);
    }

    private function writeToError(string $message): void
    {
        fwrite(STDERR, $message . PHP_EOL);
    }
}

$mode = $argv[1] ?? 'install';
$installer = new EngineAiFfiInstaller();

exit($installer->run($mode));
