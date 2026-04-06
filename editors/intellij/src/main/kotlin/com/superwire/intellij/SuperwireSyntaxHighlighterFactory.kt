package com.superwire.intellij

import com.intellij.openapi.fileTypes.PlainSyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.registry.Registry
import com.intellij.openapi.vfs.VirtualFile
import org.jetbrains.plugins.textmate.TextMateService
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateHighlighter
import org.jetbrains.plugins.textmate.language.syntax.lexer.TextMateHighlightingLexer

class SuperwireSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter {
        val languageDescriptor = resolveLanguageDescriptor(virtualFile)

        if (languageDescriptor == null) {
            return PlainSyntaxHighlighter()
        }

        val lineHighlightingLimit = Registry.get("textmate.line.highlighting.limit").asInteger()
        val textMateLexer = TextMateHighlightingLexer(languageDescriptor, lineHighlightingLimit)

        return TextMateHighlighter(textMateLexer)
    }

    private fun resolveLanguageDescriptor(virtualFile: VirtualFile?): org.jetbrains.plugins.textmate.language.TextMateLanguageDescriptor? {
        val textMateService = TextMateService.getInstance()
        val fileExtension = virtualFile?.extension ?: SuperwirePluginConstants.FILE_EXTENSION
        val extensionWithLeadingDot = ".${fileExtension.trimStart('.')}"

        val descriptorByFileExtension = textMateService.getLanguageDescriptorByExtension(fileExtension)

        if (descriptorByFileExtension != null) {
            return descriptorByFileExtension
        }

        val descriptorByExtensionWithDot = textMateService.getLanguageDescriptorByExtension(extensionWithLeadingDot)

        if (descriptorByExtensionWithDot != null) {
            return descriptorByExtensionWithDot
        }

        return virtualFile?.name?.let { textMateService.getLanguageDescriptorByFileName(it) }
    }
}
