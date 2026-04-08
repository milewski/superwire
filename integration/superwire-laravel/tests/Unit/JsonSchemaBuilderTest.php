<?php

namespace Superwire\Laravel\Tests\Unit;

use JsonException;
use Superwire\Laravel\Schema\JsonSchemaBuilder;
use Superwire\Laravel\Tests\TestCase;

final class JsonSchemaBuilderTest extends TestCase
{
    /**
     * @throws JsonException
     */
    public function testBuildsObjectSchemaWithTypedProperties(): void
    {
        $schema = JsonSchemaBuilder::object()
            ->property('city', JsonSchemaBuilder::nullableString())
            ->property('summary', JsonSchemaBuilder::string())
            ->required([ 'summary' ])
            ->toArray();

        $this->assertSame('object', $schema[ 'type' ]);
        $this->assertSame([ 'string', 'null' ], $schema[ 'properties' ][ 'city' ][ 'type' ]);
        $this->assertSame('string', $schema[ 'properties' ][ 'summary' ][ 'type' ]);
        $this->assertSame([ 'summary' ], $schema[ 'required' ]);
    }
}
