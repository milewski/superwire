<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Tests\Fixtures;

use Attribute;
use Spatie\LaravelData\Attributes\DataCollectionOf;
use Spatie\LaravelData\Data;
use Spatie\LaravelData\DataCollection;
use Superwire\Laravel\Contracts\ToolBoundInputData;
use Superwire\Laravel\Contracts\ToolInputData;
use Superwire\Laravel\Tools\AbstractTool;

#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY | Attribute::TARGET_PARAMETER)]
final readonly class Description
{
    public function __construct(public string $text)
    {
    }
}

final class DescribedTool extends AbstractTool
{
    public static function description(): string
    {
        return 'Fixture tool with field descriptions';
    }

    protected function handle(DescribedToolAgentInput $agentInput, DescribedToolBoundInput $boundInput): DescribedToolOutput
    {
        return new DescribedToolOutput(result: 'ok');
    }
}

#[Description('Input payload with translation entries.')]
final class DescribedToolAgentInput extends Data implements ToolInputData
{
    /**
     * @param DataCollection<int, DescribedTranslationEntry> $name
     */
    public function __construct(
        #[Description('Localized name entries. One per supported language.')]
        #[DataCollectionOf(DescribedTranslationEntry::class)]
        public DataCollection $name,
    )
    {
    }
}

final class DescribedToolBoundInput extends Data implements ToolBoundInputData
{
    public function __construct()
    {
    }
}

#[Description('Single localized translation entry.')]
final class DescribedTranslationEntry extends Data
{
    public function __construct(
        #[Description('Language code such as en_US or zh_CN.')]
        public string $language,
        #[Description('Localized text for the selected language.')]
        public string $value,
    )
    {
    }
}

final class DescribedToolOutput extends Data
{
    public function __construct(public string $result)
    {
    }
}
