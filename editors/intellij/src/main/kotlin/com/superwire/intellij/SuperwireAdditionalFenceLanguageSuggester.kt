package com.superwire.intellij

import com.intellij.lang.Language
import org.intellij.plugins.markdown.injection.aliases.AdditionalFenceLanguageSuggester

class SuperwireAdditionalFenceLanguageSuggester : AdditionalFenceLanguageSuggester {
    override fun suggestLanguage(name: String): Language? {
        val normalizedLanguageName = name
            .trim()
            .substringBefore(' ')
            .substringBefore('{')
            .lowercase()

        return if (normalizedLanguageName in supportedFenceTags) {
            SuperwireLanguage
        } else {
            null
        }
    }

    private companion object {
        val supportedFenceTags = setOf(
            "ai",
            "superwire",
            "ai-dsl",
        )
    }
}
