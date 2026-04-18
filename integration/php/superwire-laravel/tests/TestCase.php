<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests;

use Orchestra\Testbench\TestCase as OrchestraTestCase;
use Prism\Prism\PrismServiceProvider;
use Superwire\Laravel\SuperwireLaravelServiceProvider;

abstract class TestCase extends OrchestraTestCase
{
    /**
     * @param \Illuminate\Foundation\Application $app
     * @return list<class-string>
     */
    protected function getPackageProviders($app): array
    {
        return [
            PrismServiceProvider::class,
            SuperwireLaravelServiceProvider::class,
        ];
    }
}
