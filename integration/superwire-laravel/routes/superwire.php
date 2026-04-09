<?php

declare(strict_types = 1);

use Illuminate\Support\Facades\Route;
use Superwire\Laravel\Http\Controllers\InternalToolController;

$routePrefix = trim((string) config('superwire.tools.http_prefix', 'superwire/tools'), '/');
$routeMiddleware = config('superwire.routes.middleware', [ 'api' ]);

Route::middleware($routeMiddleware)->post(sprintf('/%s/{tool}/execute', $routePrefix), InternalToolController::class);
