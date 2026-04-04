package com.engineai.intellij

import com.intellij.openapi.fileTypes.PlainSyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.registry.Registry
import com.intellij.openapi.vfs.VirtualFile
import org.jetbrains.plugins.textmate.TextMateService
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateHighlighter
import org.jetbrains.plugins.textmate.language.syntax.lexer.TextMateHighlightingLexer

class EngineAiSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter {
        val textMateService = TextMateService.getInstance()
        val languageDescriptor = textMateService.getLanguageDescriptorByExtension(EngineAiPluginConstants.FILE_EXTENSION)

        if (languageDescriptor == null) {
            return PlainSyntaxHighlighter()
        }

        val lineHighlightingLimit = Registry.get("textmate.line.highlighting.limit").asInteger()
        val textMateLexer = TextMateHighlightingLexer(languageDescriptor, lineHighlightingLimit)

        return TextMateHighlighter(textMateLexer)
    }
}
