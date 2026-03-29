<?php

declare(strict_types=1);

namespace EngineAi\Ffi\Composer;

use Composer\Composer;
use Composer\DependencyResolver\Operation\InstallOperation;
use Composer\DependencyResolver\Operation\UpdateOperation;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Installer\PackageEvent;
use Composer\Installer\PackageEvents;
use Composer\Package\PackageInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

final class NativeInstallerPlugin implements PluginInterface, EventSubscriberInterface
{
    private const PACKAGE_NAME = 'engine-ai/ffi-php-bridge';

    private IOInterface $io;

    private bool $hasAttemptedNativeInstall;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->io = $io;
        $this->hasAttemptedNativeInstall = false;
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }

    /**
     * @return array<string, string>
     */
    public static function getSubscribedEvents(): array
    {
        return [
            PackageEvents::POST_PACKAGE_INSTALL => 'onPostPackageInstall',
            PackageEvents::POST_PACKAGE_UPDATE => 'onPostPackageUpdate',
            ScriptEvents::POST_INSTALL_CMD => 'onPostInstallCommand',
            ScriptEvents::POST_UPDATE_CMD => 'onPostUpdateCommand',
        ];
    }

    public function onPostPackageInstall(PackageEvent $event): void
    {
        $operation = $event->getOperation();

        if (!$operation instanceof InstallOperation) {
            return;
        }

        if (!$this->isTargetPackage($operation->getPackage())) {
            return;
        }

        $this->runNativeInstallerScript();
    }

    public function onPostPackageUpdate(PackageEvent $event): void
    {
        $operation = $event->getOperation();

        if (!$operation instanceof UpdateOperation) {
            return;
        }

        if (!$this->isTargetPackage($operation->getTargetPackage())) {
            return;
        }

        $this->runNativeInstallerScript();
    }

    public function onPostInstallCommand(Event $event): void
    {
        $this->runNativeInstallerScript();
    }

    public function onPostUpdateCommand(Event $event): void
    {
        $this->runNativeInstallerScript();
    }

    private function isTargetPackage(PackageInterface $package): bool
    {
        return $package->getName() === self::PACKAGE_NAME;
    }

    private function runNativeInstallerScript(): void
    {
        if ($this->hasAttemptedNativeInstall) {
            return;
        }

        $this->hasAttemptedNativeInstall = true;
        $packageRootDirectory = \dirname(__DIR__, 2);
        $installerScriptPath = $packageRootDirectory . '/scripts/install-native.php';

        if (!\is_file($installerScriptPath)) {
            $this->io->writeError(
                '<warning>[engine-ai/ffi-php-bridge] Native installer script not found; skipping extension installation.</warning>',
            );

            return;
        }

        $this->io->writeError('<info>[engine-ai/ffi-php-bridge] Installing native extension...</info>');
        $exitCode = $this->runPhpScript($installerScriptPath, $packageRootDirectory);

        if ($exitCode !== 0) {
            $this->io->writeError(
                '<warning>[engine-ai/ffi-php-bridge] Native installer finished with non-zero exit code.</warning>',
            );
        }
    }

    private function runPhpScript(string $scriptPath, string $workingDirectory): int
    {
        $descriptorSpecification = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ];

        $process = \proc_open([PHP_BINARY, $scriptPath], $descriptorSpecification, $pipes, $workingDirectory);

        if (!\is_resource($process)) {
            $this->io->writeError(
                '<warning>[engine-ai/ffi-php-bridge] Could not start native installer process.</warning>',
            );

            return 1;
        }

        \fclose($pipes[0]);
        $standardOutput = \stream_get_contents($pipes[1]);
        $standardError = \stream_get_contents($pipes[2]);
        \fclose($pipes[1]);
        \fclose($pipes[2]);

        $exitCode = \proc_close($process);

        if ($standardOutput !== false && \trim($standardOutput) !== '') {
            $this->io->write($standardOutput, false, IOInterface::NORMAL);
        }

        if ($standardError !== false && \trim($standardError) !== '') {
            $this->io->writeError($standardError, false, IOInterface::NORMAL);
        }

        return $exitCode;
    }
}
