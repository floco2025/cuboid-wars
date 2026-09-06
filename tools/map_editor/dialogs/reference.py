from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt

from PySide6.QtWidgets import QDialog, QDialogButtonBox, QTextBrowser, QVBoxLayout, QWidget


class ToolReferenceDialog(QDialog):
    """Markdown-rendered reference dialog for the editor's tools and
    shortcuts. Content lives in `tool_reference.md` next to this module so it
    can be edited without touching Python. Non-modal so the user can keep it
    open as a side panel while working — re-invoking Help → Tool Reference
    raises the existing window and re-reads the file rather than stacking
    duplicate dialogs."""

    REFERENCE_PATH = Path(__file__).resolve().parents[1] / "tool_reference.md"
    PARENT_ATTR = "_tool_reference_dialog"

    def __init__(self, parent: QWidget):
        super().__init__(parent)
        self.setWindowTitle("Tool Reference")
        self.resize(640, 560)
        # Non-modal so it can sit next to the main window while editing.
        self.setModal(False)
        # Promote to a regular top-level window. The default Qt.Dialog window
        # type drops the resize affordance on some platforms (notably macOS,
        # where it renders as a non-resizable panel). Qt.Window gives it full
        # window-manager decorations including resize handles.
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.Window)
        # Belt-and-braces: explicit resize grip so the corner is reachable
        # even when the title bar's resize chrome is platform-dependent.
        self.setSizeGripEnabled(True)

        self._browser = QTextBrowser(self)
        self._browser.setOpenExternalLinks(False)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(self.reject)
        buttons.button(QDialogButtonBox.StandardButton.Close).clicked.connect(self.accept)

        layout = QVBoxLayout(self)
        layout.addWidget(self._browser)
        layout.addWidget(buttons)

        self.reload_content()

    def reload_content(self) -> None:
        try:
            markdown = self.REFERENCE_PATH.read_text(encoding="utf-8")
        except OSError as exc:
            markdown = f"# Tool Reference\n\nFailed to read `{self.REFERENCE_PATH}`:\n\n```\n{exc}\n```"
        self._browser.setMarkdown(markdown)

    @classmethod
    def open_for(cls, parent: QWidget) -> "ToolReferenceDialog":
        # Reuse a single instance per parent — opening Help → Tool Reference
        # while it's already visible should bring the same window forward,
        # not spawn duplicates. Re-read the .md on each invocation so edits
        # are picked up without restarting the editor.
        existing = getattr(parent, cls.PARENT_ATTR, None)
        if existing is None:
            existing = cls(parent)
            setattr(parent, cls.PARENT_ATTR, existing)
        existing.reload_content()
        existing.show()
        existing.raise_()
        existing.activateWindow()
        return existing
