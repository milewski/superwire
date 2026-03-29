<?php

declare(strict_types=1);

namespace EngineAi\Ffi\Examples\Tools;

use EngineAi\Ffi\Schema;
use EngineAi\Ffi\Tool;

final class SlugifyTitleTool extends Tool
{
    public function description(): string
    {
        return 'Converts a title into a lowercase slug';
    }

    public function inputSchema(): array
    {
        return Schema::object([
            'title' => Schema::string(),
        ]);
    }

    public function outputSchema(): ?array
    {
        return Schema::object([
            'slug' => Schema::string(),
        ]);
    }

    public function execute(array $toolArguments): array
    {
        $input = \is_array($toolArguments['input'] ?? null) ? $toolArguments['input'] : [];
        $boundedArguments = \is_array($toolArguments['bounded'] ?? null) ? $toolArguments['bounded'] : [];

        $title = \is_string($input['title'] ?? null) ? $input['title'] : '';
        $prefix = \is_string($boundedArguments['prefix'] ?? null) ? $boundedArguments['prefix'] : 'article';

        $normalizedTitle = \strtolower($title);
        $slug = \preg_replace('/[^a-z0-9]+/', '-', $normalizedTitle);
        $slug = \trim((string) $slug, '-');

        return [
            'slug' => "{$prefix}-{$slug}",
        ];
    }
}
