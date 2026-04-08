<?php

namespace Superwire\Laravel\Tests;

use Orchestra\Testbench\TestCase as OrchestraTestCase;
use Superwire\Laravel\SuperwireServiceProvider;
use Superwire\Laravel\Tests\Concerns\AssertsToolSchemas;

abstract class TestCase extends OrchestraTestCase
{
    use AssertsToolSchemas;

    /**
     * @var list<string>
     */
    private array $temporaryDirectories = [];

    /**
     * @param \Illuminate\Contracts\Foundation\Application $application
     * @return array<int, class-string>
     */
    protected function getPackageProviders($application): array
    {
        return [
            SuperwireServiceProvider::class,
        ];
    }

    /**
     * @param \Illuminate\Contracts\Foundation\Application $application
     */
    protected function defineEnvironment($application): void
    {
        $application['config']->set('superwire.runtime.internal_token', 'test-internal-token');
        $application['config']->set('superwire.routes.middleware', []);
        $application['config']->set('superwire.security.enforce_localhost_only', false);
    }

    protected function createTemporaryDirectory(string $prefix): string
    {
        $baseTemporaryPath = sys_get_temp_dir();
        $temporaryDirectory = $baseTemporaryPath . DIRECTORY_SEPARATOR . sprintf(
            '%s-%d-%s',
            $prefix,
            getmypid(),
            bin2hex(random_bytes(6)),
        );

        mkdir($temporaryDirectory, 0777, true);

        $this->temporaryDirectories[] = $temporaryDirectory;

        return $temporaryDirectory;
    }

    protected function tearDown(): void
    {
        foreach ($this->temporaryDirectories as $temporaryDirectory) {
            $this->removeDirectory($temporaryDirectory);
        }

        parent::tearDown();
    }

    private function removeDirectory(string $directoryPath): void
    {
        if (!is_dir($directoryPath)) {
            return;
        }

        $entries = scandir($directoryPath);

        if (!is_array($entries)) {
            return;
        }

        foreach ($entries as $entry) {
            if ($entry === '.' || $entry === '..') {
                continue;
            }

            $entryPath = $directoryPath . DIRECTORY_SEPARATOR . $entry;

            if (is_dir($entryPath)) {
                $this->removeDirectory($entryPath);

                continue;
            }

            unlink($entryPath);
        }

        rmdir($directoryPath);
    }
}
