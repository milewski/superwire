<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Unit;

use Superwire\Laravel\Execution\Compiler\ToolNameFormatter;
use Superwire\Laravel\Tests\TestCase;

final class ToolNameFormatterTest extends TestCase
{
    public function testBuildsModuleNameFromToolName(): void
    {
        $toolNameFormatter = new ToolNameFormatter();

        $moduleName = $toolNameFormatter->moduleName('get-task_by-id');

        $this->assertSame('get_task_by_id', $moduleName);
    }

    public function testBuildsRustTypeNameFromToolName(): void
    {
        $toolNameFormatter = new ToolNameFormatter();

        $typeName = $toolNameFormatter->typeName('get-task_by-id');

        $this->assertSame('GetTaskById', $typeName);
    }
}
