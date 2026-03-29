<?php

declare(strict_types = 1);

namespace EngineAi\Ffi\Examples\Tools;

use EngineAi\Ffi\Attributes\Description;
use EngineAi\Ffi\Tool;

final class SlugifyTitleTool extends Tool
{
    public string $input = SlugifyTitleInput::class;

    public string $bounded = SlugifyTitleBounded::class;

    public function description(): string
    {
        return 'Converts a title into a lowercase slug';
    }

    public function execute(SlugifyTitleInput $input, SlugifyTitleBounded $bounded): SlugifyTitleOutput
    {
        $title = $input->title;
        $prefix = $bounded->prefix;

        $normalizedTitle = strtolower($title);
        $slug = preg_replace('/[^a-z0-9]+/', '-', $normalizedTitle);
        $slug = trim((string) $slug, '-');

        return new SlugifyTitleOutput(
            slug: "$prefix-$slug",
        );
    }
}

final class SlugifyTitleOutput
{
    public function __construct(
        #[Description('Lowercase slug generated from the given title.')]
        public string $slug,
    ) {
    }
}

final class SlugifyTitleInput
{
    public function __construct(
        #[Description('Title text to transform into a slug.')]
        public string $title,
    ) {
    }
}

final class SlugifyTitleBounded
{
    public function __construct(
        public string $prefix = 'article',
    ) {
    }
}
