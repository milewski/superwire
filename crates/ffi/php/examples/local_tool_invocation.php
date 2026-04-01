<?php

declare(strict_types = 1);

use EngineAi\Ffi\Engine;
use EngineAi\Ffi\Examples\Tools\SlugifyTitleTool;

require __DIR__ . '/../vendor/autoload.php';
require __DIR__ . '/tools/SlugifyTitleTool.php';

$engine = new Engine();

$engine->registerGlobalTool(new SlugifyTitleTool(), [
    'bounded' => [
        'prefix' => 'news',
    ],
]);

$result = $engine->invokeTool('slugify_title_tool', [
    'title' => 'Quarterly Product Update #7',
]);

print_r($result);
