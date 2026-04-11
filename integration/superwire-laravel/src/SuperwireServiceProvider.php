<?php

declare(strict_types = 1);

namespace Superwire\Laravel;

use Illuminate\Support\ServiceProvider;
use Superwire\Laravel\Console\BuildToolsCommand;
use Superwire\Laravel\Console\PrepareToolsCommand;
use Superwire\Laravel\Console\RunWorkflowCommand;
use Superwire\Laravel\Execution\ToolCompiler;
use Superwire\Laravel\Execution\WorkflowExecutor;
use Superwire\Laravel\Support\OutputMapper;
use Superwire\Laravel\Support\ToolRegistry;

class SuperwireServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__ . '/../config/superwire.php', 'superwire');

        $this->app->singleton(WorkflowExecutor::class);
        $this->app->singleton(ToolCompiler::class);
        $this->app->singleton(OutputMapper::class);
        $this->app->singleton(ToolRegistry::class);
    }

    public function boot(): void
    {
        $this->publishes([
            __DIR__ . '/../config/superwire.php' => config_path('superwire.php'),
        ], 'superwire-config');

        $this->loadRoutesFrom(__DIR__ . '/../routes/superwire.php');

        if ($this->app->runningInConsole()) {

            $this->commands([
                BuildToolsCommand::class,
                PrepareToolsCommand::class,
                RunWorkflowCommand::class,
            ]);

        }
    }
}
