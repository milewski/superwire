package com.engineai.intellij

import com.intellij.lang.ParserDefinition
import com.intellij.openapi.fileTypes.PlainTextParserDefinition

class EngineAiParserDefinition : ParserDefinition by PlainTextParserDefinition()
