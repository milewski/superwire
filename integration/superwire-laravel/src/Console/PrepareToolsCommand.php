<?php

declare(strict_types = 1);

namespace Superwire\Laravel\Console;

use Illuminate\Console\Command;
use RecursiveDirectoryIterator;
use RecursiveIteratorIterator;
use ReflectionClass;
use Superwire\Laravel\Contracts\Tool;
use Superwire\Laravel\Data\ToolBuildRequest;
use Superwire\Laravel\Exceptions\ToolBuildException;
use Superwire\Laravel\Execution\ToolCompiler;
use Superwire\Laravel\Wit\Schema\WitSchemaRecordKind;
use Superwire\Laravel\Wit\WitToolSchemaParser;
use Throwable;

final class PrepareToolsCommand extends Command
{
    protected $signature = 'superwire:tools:prepare
        {scan-path? : Directory to scan for workflows and tool classes}
        {--workflow=* : Workflow files to prepare (defaults to scanning for .wire files)}
        {--tool=* : Fully-qualified PHP tool classes to compile (defaults to superwire.tools.registered_classes)}';

    protected $description = 'Scan workflows and publish compiled tool wasm artifacts before runtime execution';

    public function __construct(private readonly ToolCompiler $toolCompiler)
    {
        parent::__construct();
    }

    public function handle(): int
    {
        $scanRootDirectory = $this->scanRootDirectory();

        $this->line(sprintf('Scanning workflows in `%s`...', $scanRootDirectory));

        $workflowFilePaths = $this->workflowFilePaths();

        if ($workflowFilePaths === []) {

            $this->warn('No workflow files found to prepare.');

            return self::SUCCESS;

        }

        $this->info(sprintf('Found %d workflow file(s).', count($workflowFilePaths)));

        $referencedToolNamesByWorkflow = $this->referencedToolNamesByWorkflow($workflowFilePaths);
        $referencedToolNames = [];

        foreach ($referencedToolNamesByWorkflow as $referencedToolNamesForWorkflow) {
            $referencedToolNames = array_merge($referencedToolNames, $referencedToolNamesForWorkflow);
        }

        $referencedToolNames = array_values(array_unique($referencedToolNames));
        sort($referencedToolNames);

        if ($referencedToolNames === []) {

            $this->info(sprintf('No tool references found across %d workflow file(s).', count($workflowFilePaths)));

            return self::SUCCESS;

        }

        $this->info(sprintf('Found %d unique tool reference(s).', count($referencedToolNames)));

        $toolClasses = $this->configuredToolClasses();

        if ($toolClasses !== []) {
            $this->line(sprintf('Using %d configured tool class(es).', count($toolClasses)));
        }

        if ($toolClasses === []) {

            $this->line('No configured tool classes found, discovering tool classes...');
            $toolClasses = $this->discoverToolClassesForNames($referencedToolNames);

            $this->line(sprintf('Discovered %d candidate tool class(es).', count($toolClasses)));

        }

        if ($toolClasses === []) {

            $this->error(
                'No tool classes provided and auto-discovery did not find matching tool implementations. '
                . 'Use --tool=App\\Superwire\\Tools\\WeatherTool or configure superwire.tools.registered_classes.',
            );

            return self::FAILURE;

        }

        $toolClassesByName = $this->toolClassesByName($toolClasses);

        $unknownToolNames = array_values(array_filter(
            $referencedToolNames,
            static fn (string $toolName): bool => !array_key_exists($toolName, $toolClassesByName),
        ));

        if ($unknownToolNames !== []) {

            $generatedToolClasses = $this->scaffoldMissingToolClassesFromWit($unknownToolNames);

            if ($generatedToolClasses !== []) {

                $this->line(sprintf('Generated %d tool class stub(s) from WIT.', count($generatedToolClasses)));
                $toolClasses = array_values(array_unique(array_merge($toolClasses, $generatedToolClasses)));
                $toolClassesByName = $this->toolClassesByName($toolClasses);
                $unknownToolNames = array_values(array_filter(
                    $referencedToolNames,
                    static fn (string $toolName): bool => !array_key_exists($toolName, $toolClassesByName),
                ));

            }

        }

        if ($unknownToolNames !== []) {

            $this->error(sprintf(
                'Workflow references unknown tool(s): %s. Add matching classes via --tool or superwire.tools.registered_classes.',
                implode(', ', array_map(static fn (string $toolName): string => sprintf('tool.%s', $toolName), $unknownToolNames)),
            ));

            return self::FAILURE;

        }

        $toolClassesToBuild = array_values(array_unique(array_map(
            fn (string $toolName): string => $toolClassesByName[ $toolName ],
            $referencedToolNames,
        )));

        $this->line(sprintf('Compiling %d tool module(s)...', count($toolClassesToBuild)));

        $buildResult = $this->toolCompiler->build(new ToolBuildRequest($toolClassesToBuild));
        $this->line(sprintf('Compilation finished. Artifacts available at `%s`.', $buildResult->outputDirectory));

        $publishedArtifactCount = 0;

        foreach ($referencedToolNamesByWorkflow as $workflowFilePath => $referencedToolNamesForWorkflow) {

            $this->line(sprintf(
                'Publishing %d artifact(s) to `%s`...',
                count($referencedToolNamesForWorkflow),
                dirname($workflowFilePath) . DIRECTORY_SEPARATOR . 'tools',
            ));

            $publishedArtifactCount += $this->publishArtifactsToWorkflow($buildResult->outputDirectory, $workflowFilePath, $referencedToolNamesForWorkflow);

        }

        $this->info(sprintf(
            'Prepared %d tool artifact(s) across %d workflow file(s).',
            $publishedArtifactCount,
            count($workflowFilePaths),
        ));

        return self::SUCCESS;
    }

    /**
     * @return list<string>
     */
    private function workflowFilePaths(): array
    {
        $workflowOptions = $this->option('workflow');

        if (is_array($workflowOptions) && $workflowOptions !== []) {

            $workflowFilePaths = array_values(array_filter(
                $workflowOptions,
                static fn (mixed $workflowPath): bool => is_string($workflowPath) && $workflowPath !== '',
            ));

            sort($workflowFilePaths);

            return $workflowFilePaths;

        }

        $scanRoot = $this->scanRootDirectory();

        return $this->discoverWorkflowFilePaths($scanRoot);
    }

    private function scanRootDirectory(): string
    {
        $scanPathArgument = $this->argument('scan-path');

        if (is_string($scanPathArgument) && $scanPathArgument !== '') {

            if (str_starts_with($scanPathArgument, DIRECTORY_SEPARATOR)) {

                $normalizedAbsolutePath = realpath($scanPathArgument);

                return $normalizedAbsolutePath === false ? $scanPathArgument : $normalizedAbsolutePath;

            }

            $workingDirectoryPath = getcwd() ?: base_path();
            $workingDirectoryCandidate = $workingDirectoryPath . DIRECTORY_SEPARATOR . $scanPathArgument;
            $normalizedWorkingDirectoryCandidate = realpath($workingDirectoryCandidate);

            if ($normalizedWorkingDirectoryCandidate !== false && is_dir($normalizedWorkingDirectoryCandidate)) {
                return $normalizedWorkingDirectoryCandidate;
            }

            $basePathCandidate = base_path($scanPathArgument);
            $normalizedBasePathCandidate = realpath($basePathCandidate);

            if ($normalizedBasePathCandidate !== false) {
                return $normalizedBasePathCandidate;
            }

            return $basePathCandidate;

        }

        return (string) config('superwire.cli.working_directory', base_path());
    }

    /**
     * @return list<string>
     */
    private function discoverWorkflowFilePaths(string $scanRoot): array
    {
        if (!is_dir($scanRoot)) {
            return [];
        }

        $workflowFilePaths = [];
        $directoryIterator = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($scanRoot, RecursiveDirectoryIterator::SKIP_DOTS),
        );

        foreach ($directoryIterator as $fileInfo) {

            $absolutePath = $fileInfo->getPathname();

            if ($this->shouldSkipPath($absolutePath)) {
                continue;
            }

            if (!$fileInfo->isFile()) {
                continue;
            }

            if ($fileInfo->getExtension() !== 'wire') {
                continue;
            }

            $workflowFilePaths[] = $absolutePath;

        }

        sort($workflowFilePaths);

        return $workflowFilePaths;
    }

    private function shouldSkipPath(string $absolutePath): bool
    {
        $baseName = basename($absolutePath);

        if (
            str_starts_with($baseName, '.')
            || str_starts_with($baseName, '_')
            || str_contains($baseName, 'ide_helper')
        ) {
            return true;
        }

        $pathSegments = explode(DIRECTORY_SEPARATOR, $absolutePath);

        foreach ($pathSegments as $pathSegment) {

            if ($pathSegment === '.' || $pathSegment === '..') {
                continue;
            }

            if ($pathSegment !== '' && str_starts_with($pathSegment, '.')) {
                return true;
            }

        }

        $excludedPathSegments = [
            DIRECTORY_SEPARATOR . 'vendor' . DIRECTORY_SEPARATOR,
            DIRECTORY_SEPARATOR . 'node_modules' . DIRECTORY_SEPARATOR,
            DIRECTORY_SEPARATOR . '.git' . DIRECTORY_SEPARATOR,
        ];

        foreach ($excludedPathSegments as $excludedPathSegment) {

            if (str_contains($absolutePath, $excludedPathSegment)) {
                return true;
            }

        }

        return false;
    }

    /**
     * @return list<class-string<Tool>>
     */
    private function configuredToolClasses(): array
    {
        $toolOptions = $this->option('tool');

        if (is_array($toolOptions) && $toolOptions !== []) {

            return array_values(array_filter(
                $toolOptions,
                static fn (mixed $toolClass): bool => is_string($toolClass) && $toolClass !== '',
            ));

        }

        $configuredToolClasses = config('superwire.tools.registered_classes', []);

        if (!is_array($configuredToolClasses)) {
            return [];
        }

        return array_values(array_filter(
            $configuredToolClasses,
            static fn (mixed $toolClass): bool => is_string($toolClass) && $toolClass !== '',
        ));
    }

    /**
     * @param list<string> $toolNames
     * @return list<class-string<Tool>>
     */
    private function discoverToolClassesForNames(array $toolNames): array
    {
        $scanRoots = [ $this->scanRootDirectory() ];
        $applicationRoot = app_path();

        if (is_string($applicationRoot) && $applicationRoot !== '' && !in_array($applicationRoot, $scanRoots, true)) {
            $scanRoots[] = $applicationRoot;
        }

        $discoveredToolClasses = [];
        $desiredToolNames = array_fill_keys($toolNames, true);

        foreach ($scanRoots as $scanRoot) {

            if (!is_dir($scanRoot)) {
                continue;
            }

            foreach ($this->projectPhpFilePaths($scanRoot) as $phpFilePath) {

                foreach ($this->classNamesFromPhpFile($phpFilePath) as $className) {

                    $resolvedToolClassName = $this->resolveToolClassName($className, $phpFilePath);

                    if ($resolvedToolClassName === null) {
                        continue;
                    }

                    $toolName = $resolvedToolClassName::name();

                    if (!isset($desiredToolNames[ $toolName ])) {
                        continue;
                    }

                    $discoveredToolClasses[] = $resolvedToolClassName;

                }

            }

        }

        $discoveredToolClasses = array_values(array_unique($discoveredToolClasses));
        sort($discoveredToolClasses);

        return $discoveredToolClasses;
    }

    /**
     * @return list<string>
     */
    private function projectPhpFilePaths(string $scanRoot): array
    {
        $phpFilePaths = [];
        $directoryIterator = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($scanRoot, RecursiveDirectoryIterator::SKIP_DOTS),
        );

        foreach ($directoryIterator as $fileInfo) {

            $absolutePath = $fileInfo->getPathname();

            if ($this->shouldSkipPath($absolutePath)) {
                continue;
            }

            if (!$fileInfo->isFile()) {
                continue;
            }

            if ($fileInfo->getExtension() !== 'php') {
                continue;
            }

            $isToolFileName = str_ends_with($fileInfo->getFilename(), 'Tool.php');
            $isInsideToolsDirectory = str_contains(
                $absolutePath,
                DIRECTORY_SEPARATOR . 'Tools' . DIRECTORY_SEPARATOR,
            );

            if (!$isToolFileName && !$isInsideToolsDirectory) {
                continue;
            }

            $phpFilePaths[] = $absolutePath;

        }

        sort($phpFilePaths);

        return $phpFilePaths;
    }

    /**
     * @return list<class-string>
     */
    private function classNamesFromPhpFile(string $phpFilePath): array
    {
        $source = (string) file_get_contents($phpFilePath);

        if ($source === '') {
            return [];
        }

        $tokens = token_get_all($source);
        $namespace = '';
        $classNames = [];

        for ($tokenIndex = 0; $tokenIndex < count($tokens); $tokenIndex++) {

            $token = $tokens[ $tokenIndex ];

            if (!is_array($token)) {
                continue;
            }

            if ($token[ 0 ] === T_NAMESPACE) {

                $namespace = $this->readNamespace($tokens, $tokenIndex + 1);

                continue;

            }

            if ($token[ 0 ] !== T_CLASS) {
                continue;
            }

            $className = $this->readClassName($tokens, $tokenIndex + 1);

            if ($className === null) {
                continue;
            }

            $classNames[] = $namespace === '' ? $className : $namespace . '\\' . $className;

        }

        return $classNames;
    }

    /**
     * @param list<mixed> $tokens
     */
    private function readNamespace(array $tokens, int $startIndex): string
    {
        $namespace = '';

        for ($tokenIndex = $startIndex; $tokenIndex < count($tokens); $tokenIndex++) {

            $token = $tokens[ $tokenIndex ];

            if (!is_array($token)) {

                if ($token === ';' || $token === '{') {
                    break;
                }

                continue;

            }

            if (
                $token[ 0 ] === T_STRING
                || $token[ 0 ] === T_NS_SEPARATOR
                || $token[ 0 ] === T_NAME_QUALIFIED
                || $token[ 0 ] === T_NAME_FULLY_QUALIFIED
                || $token[ 0 ] === T_NAME_RELATIVE
            ) {
                $namespace .= $token[ 1 ];
            }

        }

        return $namespace;
    }

    /**
     * @param list<mixed> $tokens
     */
    private function readClassName(array $tokens, int $startIndex): ?string
    {
        for ($tokenIndex = $startIndex; $tokenIndex < count($tokens); $tokenIndex++) {

            $token = $tokens[ $tokenIndex ];

            if (!is_array($token)) {

                if ($token === '{' || $token === '(') {
                    return null;
                }

                continue;

            }

            if ($token[ 0 ] === T_STRING) {
                return $token[ 1 ];
            }

            if ($token[ 0 ] !== T_WHITESPACE) {
                return null;
            }

        }

        return null;
    }

    /**
     * @param class-string $className
     * @return class-string<Tool>|null
     */
    private function resolveToolClassName(string $className, string $phpFilePath): ?string
    {
        if (!class_exists($className, false)) {

            try {

                require_once $phpFilePath;

            } catch (Throwable) {

                return null;

            }

            if (!class_exists($className, false)) {
                return null;
            }

        }

        if (!is_subclass_of($className, Tool::class)) {
            return null;
        }

        try {

            $reflectionClass = new ReflectionClass($className);

            if ($reflectionClass->isAbstract() || !$reflectionClass->isInstantiable()) {
                return null;
            }

        } catch (Throwable) {

            return null;

        }

        return $className;
    }

    /**
     * @param list<class-string<Tool>> $toolClasses
     * @return array<string, class-string<Tool>>
     */
    private function toolClassesByName(array $toolClasses): array
    {
        $toolClassesByName = [];

        foreach ($toolClasses as $toolClass) {

            if (!class_exists($toolClass)) {
                throw new ToolBuildException(sprintf('tool class `%s` does not exist', $toolClass));
            }

            if (!is_subclass_of($toolClass, Tool::class)) {
                throw new ToolBuildException(sprintf('tool class `%s` must implement %s', $toolClass, Tool::class));
            }

            $toolClassesByName[ $toolClass::name() ] = $toolClass;

        }

        return $toolClassesByName;
    }

    /**
     * @param list<string> $workflowFilePaths
     * @return array<string, list<string>>
     */
    private function referencedToolNamesByWorkflow(array $workflowFilePaths): array
    {
        $referencedToolNamesByWorkflow = [];

        foreach ($workflowFilePaths as $workflowFilePath) {

            $workflowSource = is_file($workflowFilePath) ? (string) file_get_contents($workflowFilePath) : '';

            if ($workflowSource === '') {

                $referencedToolNamesByWorkflow[ $workflowFilePath ] = [];

                continue;

            }

            preg_match_all('/\btool\.([A-Za-z_][A-Za-z0-9_]*)\b/', $workflowSource, $matches);
            $toolNames = isset($matches[ 1 ]) && is_array($matches[ 1 ]) ? array_values(array_unique($matches[ 1 ])) : [];

            sort($toolNames);

            $referencedToolNamesByWorkflow[ $workflowFilePath ] = $toolNames;

        }

        return $referencedToolNamesByWorkflow;
    }

    /**
     * @param list<string> $toolNames
     */
    private function publishArtifactsToWorkflow(string $buildOutputDirectory, string $workflowFilePath, array $toolNames): int
    {
        if ($toolNames === []) {
            return 0;
        }

        $workflowDirectory = dirname($workflowFilePath);
        $workflowToolsDirectory = $workflowDirectory . DIRECTORY_SEPARATOR . 'tools';

        if (!is_dir($workflowToolsDirectory) && !mkdir($workflowToolsDirectory, 0o777, true) && !is_dir($workflowToolsDirectory)) {
            throw new ToolBuildException(sprintf('failed to create workflow tools directory %s', $workflowToolsDirectory));
        }

        $publishedArtifactCount = 0;

        foreach ($toolNames as $toolName) {

            $sourcePath = $buildOutputDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';
            $destinationPath = $workflowToolsDirectory . DIRECTORY_SEPARATOR . $toolName . '.wasm';

            if (!is_file($sourcePath)) {
                throw new ToolBuildException(sprintf('compiled tool artifact not found at %s', $sourcePath));
            }

            if (!copy($sourcePath, $destinationPath)) {
                throw new ToolBuildException(sprintf('failed to copy tool artifact from %s to %s', $sourcePath, $destinationPath));
            }

            $publishedArtifactCount++;

        }

        return $publishedArtifactCount;
    }

    /**
     * @param list<string> $toolNames
     * @return list<class-string<Tool>>
     */
    private function scaffoldMissingToolClassesFromWit(array $toolNames): array
    {
        if ($toolNames === []) {
            return [];
        }

        $toolNamesLookup = array_fill_keys($toolNames, true);
        $generatedToolClasses = [];
        $witSchemaParser = new WitToolSchemaParser();

        foreach ($this->witFilePaths() as $witFilePath) {

            try {

                $witSchema = $witSchemaParser->parseFile($witFilePath);

            } catch (Throwable) {

                continue;

            }

            if (!isset($toolNamesLookup[ $witSchema->toolName ])) {
                continue;
            }

            $generatedToolClass = $this->generateToolClassStubFromWit($witFilePath, $witSchema);

            if ($generatedToolClass !== null) {
                $generatedToolClasses[] = $generatedToolClass;
            }

        }

        $generatedToolClasses = array_values(array_unique($generatedToolClasses));
        sort($generatedToolClasses);

        return $generatedToolClasses;
    }

    /**
     * @return list<string>
     */
    private function witFilePaths(): array
    {
        $scanRoots = [ $this->scanRootDirectory() ];
        $applicationRoot = app_path();

        if (is_string($applicationRoot) && $applicationRoot !== '' && !in_array($applicationRoot, $scanRoots, true)) {
            $scanRoots[] = $applicationRoot;
        }

        $witFilePaths = [];

        foreach ($scanRoots as $scanRoot) {

            if (!is_dir($scanRoot)) {
                continue;
            }

            $directoryIterator = new RecursiveIteratorIterator(
                new RecursiveDirectoryIterator($scanRoot, RecursiveDirectoryIterator::SKIP_DOTS),
            );

            foreach ($directoryIterator as $fileInfo) {

                $absolutePath = $fileInfo->getPathname();

                if ($this->shouldSkipPath($absolutePath)) {
                    continue;
                }

                if (!$fileInfo->isFile()) {
                    continue;
                }

                if ($fileInfo->getExtension() !== 'wit') {
                    continue;
                }

                $witFilePaths[] = $absolutePath;

            }

        }

        $witFilePaths = array_values(array_unique($witFilePaths));
        sort($witFilePaths);

        return $witFilePaths;
    }

    private function generateToolClassStubFromWit(string $witFilePath, object $witSchema): ?string
    {
        $toolDirectory = dirname($witFilePath);
        $applicationRoot = app_path();

        if (!str_starts_with($toolDirectory, $applicationRoot . DIRECTORY_SEPARATOR)) {
            return null;
        }

        $toolClassName = $this->pascalCase($witSchema->toolName);
        $toolClassPath = $toolDirectory . DIRECTORY_SEPARATOR . $toolClassName . '.php';

        if (is_file($toolClassPath)) {

            foreach ($this->classNamesFromPhpFile($toolClassPath) as $className) {

                if (is_subclass_of($className, Tool::class)) {
                    return $className;
                }

            }

            return null;

        }

        $relativeDirectory = trim(substr($toolDirectory, strlen($applicationRoot)), DIRECTORY_SEPARATOR);
        $namespaceSuffix = str_replace(DIRECTORY_SEPARATOR, '\\', $relativeDirectory);
        $namespace = 'App' . ($namespaceSuffix !== '' ? '\\' . $namespaceSuffix : '');

        $hasAgentInput = $witSchema->hasRecord(WitSchemaRecordKind::AgentInput);
        $hasBoundInput = $witSchema->hasRecord(WitSchemaRecordKind::BoundInput);
        $typePrefix = $this->pascalCase($witSchema->toolName);

        $parameters = [];

        if ($hasAgentInput) {
            $parameters[] = sprintf('Data\\%sAgentInput $agentInput', $typePrefix);
        }

        if ($hasBoundInput) {
            $parameters[] = sprintf('Data\\%sBoundInput $boundInput', $typePrefix);
        }

        $parameterCode = $parameters === []
            ? ''
            : "\n        " . implode(",\n        ", $parameters) . "\n    ";

        $source = "<?php\n\ndeclare(strict_types = 1);\n\nnamespace {$namespace};\n\n"
            . "use RuntimeException;\n"
            . "use Superwire\\Laravel\\Tools\\AbstractWitTool;\n\n"
            . "final class {$toolClassName} extends AbstractWitTool\n"
            . "{\n"
            . "    public static function witPath(): string\n"
            . "    {\n"
            . "        return __DIR__ . '/" . basename($witFilePath) . "';\n"
            . "    }\n\n"
            . "    protected function handle({$parameterCode}): Data\\{$typePrefix}Output\n"
            . "    {\n"
            . "        throw new RuntimeException('Implement handle() for this WIT-defined tool.');\n"
            . "    }\n"
            . "}\n";

        $written = file_put_contents($toolClassPath, $source);

        if ($written === false) {
            return null;
        }

        require_once $toolClassPath;

        $className = $namespace . '\\' . $toolClassName;

        return is_subclass_of($className, Tool::class) ? $className : null;
    }

    private function pascalCase(string $value): string
    {
        $segments = preg_split('/[^a-zA-Z0-9]+/', $value) ?: [];
        $result = '';

        foreach ($segments as $segment) {

            if ($segment === '') {
                continue;
            }

            $result .= ucfirst($segment);

        }

        if ($result === '') {
            return 'GeneratedTool';
        }

        return $result;
    }
}
