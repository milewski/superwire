package com.superwire.intellij

import com.intellij.psi.tree.IElementType
import com.intellij.psi.tree.IFileElementType

object SuperwireElementTypes {
    val FILE = IFileElementType(SuperwireLanguage)
    val SYMBOL = IElementType("SYMBOL", SuperwireLanguage)
    val TEXT = IElementType("TEXT", SuperwireLanguage)
}
