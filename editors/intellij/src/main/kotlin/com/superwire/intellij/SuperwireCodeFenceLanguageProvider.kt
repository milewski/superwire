package com.superwire.intellij

import com.intellij.codeInsight.completion.CompletionParameters
import com.intellij.codeInsight.lookup.LookupElement
import com.intellij.codeInsight.lookup.LookupElementBuilder
import com.intellij.lang.Language
import org.intellij.plugins.markdown.injection.CodeFenceLanguageProvider

class SuperwireCodeFenceLanguageProvider : CodeFenceLanguageProvider {
    override fun getLanguageByInfoString(infoString: String): Language? {
        val normalizedInfoString = normalizeInfoString(infoString)

        return if (normalizedInfoString in supportedInfoStrings) {
            SuperwireLanguage
        } else {
            null
        }
    }

    override fun getCompletionVariantsForInfoString(parameters: CompletionParameters): List<LookupElement> {
        return supportedInfoStrings
            .map(LookupElementBuilder::create)
            .toList()
    }

    private companion object {
        const val preferredInfoString = "ai"

        fun normalizeInfoString(infoString: String): String {
            return infoString
                .trim()
                .substringBefore(' ')
                .substringBefore('{')
                .lowercase()
        }

        val supportedInfoStrings = setOf(
            preferredInfoString,
            "superwire",
            "ai-dsl",
        )
    }
}
