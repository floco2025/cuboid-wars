"""The single Ramp tool: drag direction, storeys, shape, Properties, and the slope note."""

from PySide6.QtCore import QPointF, Qt
from PySide6.QtTest import QTest

from editor_fixtures import DEFAULT_ALIAS, WindowTestCase
from map_editor import constants as c
from map_editor.editing import place_ramp
from map_editor.elements import ElementRef
from map_editor.normalization import empty_level, empty_map, normalize_map


def ramp(lower, levels, cols, rows, direction="S", shape="solid"):
    return {
        "lower_level": lower,
        "levels": levels,
        "cols": cols,
        "rows": rows,
        "direction": direction,
        "shape": shape,
        "all": DEFAULT_ALIAS,
    }


class RampToolTests(WindowTestCase):
    def storeys(self, count):
        data = empty_map(8, 8)
        data["checkpoints"] = []
        data["levels"] = [empty_level(index) for index in range(count)]
        self.window.doc.replace_with_new(data)
        self.window.set_mode(c.MODE_RAMP)

    def point(self, x, y):
        return self.window.canvas.viewport.from_grid(QPointF(x, y)).toPoint()

    def drag(self, start, end):
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=self.point(*start))
        QTest.mouseMove(canvas, self.point(*end))
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=self.point(*end))

    def ramps(self):
        return [(r["cols"], r["rows"], r["direction"], r["levels"]) for r in self.window.map_data["ramps"]]

    def test_a_one_cell_ramp_rises_the_way_the_pointer_moved_and_a_still_click_repeats_it(self):
        self.storeys(2)
        self.drag((2.7, 2.5), (2.2, 2.5))
        self.assertEqual(self.ramps(), [([2, 3], [2, 3], "W", 1)])
        QTest.mouseClick(self.window.canvas, Qt.MouseButton.LeftButton, pos=self.point(5.5, 5.5))
        self.assertEqual(self.ramps()[-1], ([5, 6], [5, 6], "W", 1))

    def test_a_drag_sets_the_footprint_and_storeys_stop_at_the_top_level(self):
        self.storeys(3)
        window = self.window
        window.recent_ramp_levels = 5
        window.recent_ramp_shape = "plank"
        self.drag((1.5, 1.5), (1.5, 4.5))
        self.assertEqual(self.ramps(), [([1, 2], [1, 5], "S", 2)])
        self.assertEqual(window.map_data["ramps"][0]["shape"], "plank")
        window.set_level_index(2)
        self.drag((4.5, 1.5), (6.5, 1.5))
        self.assertEqual(len(window.map_data["ramps"]), 1, "nothing to arrive at above the top level")

    def test_properties_edit_storeys_direction_and_shape_and_refuse_a_ramp_past_the_top(self):
        self.storeys(3)
        window = self.window
        window.apply_change("Ramp", {**window.map_data, "ramps": [ramp(0, 1, [1, 2], [1, 3])]})
        window.inspect_refs([ElementRef("ramps", 0)])
        for key, value in (("levels", 2), ("direction", "N"), ("shape", "plank")):
            self.set_property(key, value)
        edited = window.map_data["ramps"][0]
        self.assertEqual((edited["levels"], edited["direction"], edited["shape"]), (2, "N", "plank"))
        self.set_property("levels", 3)
        self.assertIn("needs level 3", window.properties_panel.error.text())
        self.assertEqual(window.map_data["ramps"][0]["levels"], 2)

    def test_a_ramp_too_steep_to_climb_is_a_note_and_never_an_error(self):
        self.storeys(3)
        window = self.window
        window.grid_cell_size, window.level_height = 3.4, 4.4
        issues = len(window.document_issues().issues)
        window.apply_change("Ramp", {**window.map_data, "ramps": [ramp(0, 1, [1, 2], [1, 3])]})
        window.inspect_refs([ElementRef("ramps", 0)])
        panel = window.properties_panel
        self.assertEqual(panel.slope_note.text(), "Slope 32.9°")
        self.set_property("levels", 2)
        self.assertEqual(window.map_data["ramps"][0]["levels"], 2)
        self.assertEqual(panel.slope_note.text(), "Slope 52.3° — too steep to walk up (limit 45°)")
        self.assertTrue(panel.error.isHidden())
        self.assertEqual(len(window.document_issues().issues), issues)

    def test_a_new_ramp_replaces_one_sharing_a_storey_and_keeps_one_it_stacks_on(self):
        data = normalize_map({**empty_map(8, 8), "levels": [empty_level(i) for i in range(4)]})
        data["ramps"] = [ramp(0, 1, [1, 2], [1, 3]), ramp(1, 2, [4, 5], [1, 5])]
        stacked = place_ramp(data, ramp(1, 1, [1, 2], [1, 3]))
        self.assertEqual(len(stacked["ramps"]), 3)
        crossing = place_ramp(data, ramp(2, 1, [4, 6], [2, 3], "E"))
        self.assertEqual([r["cols"] for r in crossing["ramps"]], [[1, 2], [4, 6]])
