package com.engineai.intellij

import com.intellij.psi.tree.IElementType
import com.intellij.psi.tree.IFileElementType

object EngineAiElementTypes {
    val FILE = IFileElementType(EngineAiLanguage)
    val SYMBOL = IElementType("SYMBOL", EngineAiLanguage)
    val TEXT = IElementType("TEXT", EngineAiLanguage)
}
