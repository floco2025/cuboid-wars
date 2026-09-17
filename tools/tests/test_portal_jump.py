import copy
import unittest
from dataclasses import replace
from math import sqrt
from unittest.mock import patch

from config_fixtures import gameplay, map_settings
from editor_fixtures import WindowTestCase, floor
from map_editor.constants import MODE_FLOOR, MODE_PORTAL_JUMP, MODE_SELECT
from map_editor.jump_reach import ANTI_GRAVITY, BOTH, NORMAL, SPEED, FallSettings, JumpSettings
from map_editor.normalization import empty_level, empty_map
from map_editor.portal_jump import (
    EntryState,
    PortalSettings,
    PortalSurface,
    calculate_landings,
    crossings,
    descending_time,
    entry_states,
    exit_motion,
    feasible_times,
    traverse_vector,
)
from map_editor.portal_surfaces import PortalSurfaces, portals_overlap
from map_editor.transforms import insert_level_data, resize_map_data
from PySide6.QtCore import QPoint, QPointF, Qt
from PySide6.QtTest import QTest


class PortalJumpTests(unittest.TestCase):
    def setUp(self):
        self.settings = PortalSettings(JumpSettings(4, 5, 10, 3, 6, 2, 10, 2, 0.2, FallSettings(8, 15, 100)), 2)
        self.data = empty_map(30, 30)
        self.data["levels"] = [empty_level(i) for i in range(5)]

    def states(self, surface, *, origin=(2, 5, 5), settings=None, jumping=False, running=True, margin=0):
        return entry_states(
            settings or self.settings, origin, surface, self.data, jumping=jumping, running=running, margin=margin
        )

    def test_drop_entry_time_includes_body_center_and_independent_powerups(self):
        states = self.states(PortalSurface(0, 6, 5))
        self.assertEqual(set(states), {NORMAL, SPEED, ANTI_GRAVITY, BOTH})
        self.assertAlmostEqual(states[NORMAL][0].time, sqrt(22.02 / 10))
        self.assertAlmostEqual(states[NORMAL][0].vertical_velocity, -sqrt(220.2))
        self.assertGreater(states[ANTI_GRAVITY][0].time, states[NORMAL][0].time)
        distant = self.states(PortalSurface(0, 9, 5))
        self.assertNotIn(NORMAL, distant)
        self.assertEqual(set(distant), {SPEED, ANTI_GRAVITY, BOTH})

    def test_jump_run_and_margin_change_entry_reach(self):
        target = PortalSurface(0, 9, 5)
        self.assertIn(NORMAL, self.states(target, jumping=True))
        self.assertNotIn(NORMAL, self.states(target, jumping=True, running=False))
        self.assertNotIn(NORMAL, self.states(target, jumping=True, margin=1.5))
        self.assertEqual(self.states(target, margin=99), self.states(target))
        for invalid in (-1, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                self.states(target, margin=invalid)

    def test_level_floor_portals_return_to_takeoff_apex_for_each_powerup(self):
        settings = replace(
            self.settings,
            movement=replace(self.settings.movement, level_height=1, jump_speed=4, gravity=4, low_gravity=1),
        )
        data = empty_map(10, 10)
        data["levels"] = [empty_level(i) for i in range(14)]
        for jumping, expected in (
            (False, {NORMAL: 4, SPEED: 4, ANTI_GRAVITY: 4, BOTH: 4}),
            (True, {NORMAL: 6, SPEED: 6, ANTI_GRAVITY: 12, BOTH: 12}),
        ):
            for portal_level, turn in ((0, 0), (2, 1)):
                with self.subTest(jumping=jumping, portal_level=portal_level, turn=turn):
                    entry = PortalSurface(portal_level, 2, 2)
                    exit = PortalSurface(portal_level, 6, 6, turn=turn)
                    states = entry_states(settings, (4, 2, 2), entry, data, running=True, jumping=jumping, margin=0.1)
                    landings = calculate_landings(settings, entry, exit, states, data)
                    for bit, peak in expected.items():
                        levels = [level for (level, _, _), values in landings.items() if bit in values]
                        self.assertEqual(max(levels), peak)
                        self.assertIn(bit, landings[peak, 6, 6])

    def test_floor_requires_descending_entry_but_wall_has_both_branches(self):
        states = self.states(PortalSurface(3, 5, 5), jumping=True)
        self.assertTrue(all(s.vertical_velocity < 0 for values in states.values() for s in values))
        wall = self.states(PortalSurface(2, 6, 5, "west"), jumping=True)
        self.assertEqual(len(wall[NORMAL]), 2)
        self.assertLess(wall[NORMAL][0].vertical_velocity, 0)
        self.assertGreater(wall[NORMAL][1].vertical_velocity, 0)
        self.assertNotIn(NORMAL, self.states(PortalSurface(4, 6, 5), jumping=True))

    def test_terminal_fall_time_and_velocity(self):
        self.assertAlmostEqual(descending_time(0, 10, -250), 7.5)
        self.assertEqual(list(crossings(0, 10, -250)), [(7.5, -50)])
        self.assertAlmostEqual(descending_time(10, 10, -250), 8.6)

    def test_zero_gravity_has_no_floor_entry_or_descending_landings(self):
        settings = replace(self.settings, movement=replace(self.settings.movement, low_gravity=0))
        states = self.states(PortalSurface(0, 6, 5), settings=settings)
        self.assertEqual(set(states), {NORMAL, SPEED})
        entry, exit = PortalSurface(2, 6, 5, "west"), PortalSurface(0, 10, 10)
        states = self.states(entry, settings=settings, jumping=True)
        self.assertIn(ANTI_GRAVITY, states)
        landings = calculate_landings(settings, entry, exit, states, self.data)
        self.assertTrue(all(ANTI_GRAVITY not in values and BOTH not in values for values in landings.values()))

    def test_all_cardinal_frames_preserve_lengths_and_round_trip(self):
        surfaces = [PortalSurface(0, 4, 4, turn=t) for t in range(4)] + [
            PortalSurface(0, 4, 4, face) for face in ("north", "south", "west", "east")
        ]
        for a in surfaces:
            for b in surfaces:
                entry, exit = a.frame(self.settings), b.frame(self.settings)
                v = (3, -12, 7)
                mapped = traverse_vector(entry, exit, v)
                self.assertAlmostEqual(sum(x * x for x in mapped), sum(x * x for x in v))
                self.assertEqual(traverse_vector(exit, entry, mapped), v)

    def test_floor_orientation_follows_shot_from_start_with_game_snap(self):
        origin = (2, 5, 5)
        for col, row, expected in (
            (5, 4, 0),
            (6, 5, 1),
            (5, 6, 2),
            (4, 5, 3),
            (6, 4, 0),
            (6, 6, 1),
            (4, 6, 3),
            (4, 4, 0),
            (6, 8, 2),
            (5, 5, 0),
        ):
            with self.subTest(col=col, row=row):
                self.assertEqual(PortalSurface(0, col, row).placed_from(origin).turn, expected)
        wall = PortalSurface(0, 6, 5, "west")
        self.assertEqual(wall.placed_from(origin), wall)

    def test_floor_to_wall_carries_fall_and_control_stays_separate(self):
        entry = PortalSurface(0, 4, 4).frame(self.settings)
        state = EntryState(1, -20, 6, 10)
        for side, drift in (("east", (20, 0)), ("west", (-20, 0)), ("north", (0, -20)), ("south", (0, 20))):
            exit = PortalSurface(0, 10, 10, side).frame(self.settings)
            self.assertEqual(exit_motion(entry, exit, state), (drift, -6, 6))
        floor_exit = PortalSurface(0, 10, 10).frame(self.settings)
        self.assertEqual(exit_motion(entry, floor_exit, state), ((0, 0), 20, 20))

    def test_wall_to_floor_redirects_vertical_motion_and_inward_control(self):
        entry = PortalSurface(2, 4, 4, "west").frame(self.settings)
        state = EntryState(1, -10, 6, 10)
        exit = PortalSurface(0, 10, 10).frame(self.settings)
        self.assertEqual(exit_motion(entry, exit, state), ((0, 10), 0, 6))
        rotated = PortalSurface(0, 10, 10, turn=1).frame(self.settings)
        self.assertEqual(exit_motion(entry, rotated, state), ((-10, 0), 0, 6))
        wall = PortalSurface(0, 10, 10, "east").frame(self.settings)
        self.assertEqual(exit_motion(entry, wall, state), ((0, 0), -10, -10))

    def test_continuous_steering_envelope_cannot_erase_fast_momentum(self):
        # One second out of a wall: 20 m carried forward, up to 6 m of steering.
        self.assertTrue(list(feasible_times((0, 0), (20, 0), 6, (18, 4, 22, 5), 1, 1)))
        self.assertFalse(list(feasible_times((0, 0), (20, 0), 6, (0, -1, 1, 1), 1, 1)))
        times = list(feasible_times((0, 0), (20, 0), 6, (30, -1, 32, 1), 0.5, 3))
        self.assertAlmostEqual(min(a for a, _ in times), 30 / 26)
        self.assertAlmostEqual(max(b for _, b in times), 32 / 14)

    def test_tangent_envelope_and_stationary_center(self):
        self.assertTrue(list(feasible_times((0, 0), (0, 0), 5, (3, 4, 4, 5), 1, 1)))
        self.assertFalse(list(feasible_times((0, 0), (0, 0), 5, (3, 4.01, 4, 5), 1, 1)))
        times = list(feasible_times((0, 0), (0, 0), 5, (3, 4, 4, 5), 0.1, 2))
        self.assertAlmostEqual(min(a for a, _ in times), 1)

    def test_landings_include_safe_damaging_and_fatal_outcomes(self):
        settings = replace(self.settings, movement=replace(self.settings.movement, fall=FallSettings(2, 9, 100)))
        entry, exit = PortalSurface(0, 5, 5), PortalSurface(0, 15, 15)
        states = {NORMAL: [EntryState(1, -sqrt(200.2), 6, 10)]}
        landings = calculate_landings(settings, entry, exit, states, self.data)
        self.assertEqual(landings[0, 15, 15][NORMAL], 1)
        self.assertAlmostEqual(landings[1, 15, 15][NORMAL], 2 / 7)
        self.assertNotIn((2, 15, 15), landings)
        states[NORMAL] = [EntryState(1, -sqrt(80.2), 6, 10)]
        landings = calculate_landings(settings, entry, exit, states, self.data)
        self.assertAlmostEqual(landings[0, 15, 15][NORMAL], 1 / 7)
        states[NORMAL] = [EntryState(1, -sqrt(40), 6, 10)]
        self.assertEqual(calculate_landings(settings, entry, exit, states, self.data)[0, 15, 15][NORMAL], 0)

    def test_least_damage_can_require_an_interior_exit_velocity(self):
        settings = replace(self.settings, movement=replace(self.settings.movement, fall=FallSettings(0.1, 2, 100)))
        entry, exit = PortalSurface(0, 5, 5), PortalSurface(0, 10, 10, "east")
        states = {NORMAL: [EntryState(1, -20, 6, 10)]}
        landings = calculate_landings(settings, entry, exit, states, self.data)
        self.assertAlmostEqual(landings[0, 11, 10][NORMAL], (0.368 - 0.1) / 1.9)

    def test_surface_materials_ramps_and_planned_surfaces(self):
        target = PortalSurface(0, 5, 5)
        self.data["levels"][0]["floors"] = [dict(floor(5, 5), top="resistant")]
        surfaces = PortalSurfaces(self.data, self.settings, {"basement-floor": True, "resistant": False})
        self.assertFalse(surfaces.status(target).available)
        self.assertFalse(surfaces.status(target).planned)
        self.assertTrue(surfaces.status(PortalSurface(0, 6, 5)).planned)
        self.data["levels"][0]["floors"][0]["top"] = "basement-floor"
        surfaces = PortalSurfaces(self.data, self.settings, {"basement-floor": True})
        self.assertTrue(surfaces.status(target).available)
        self.assertFalse(surfaces.status(target).planned)
        self.data["ramps"] = [{"lower_level": 0, "low": [5, 5], "high": [6, 8]}]
        surfaces = PortalSurfaces(self.data, self.settings, {"basement-floor": True})
        self.assertIn("Ramp", surfaces.status(target).reason)

    def test_short_stacked_or_planned_walls_align_portal_rim_with_base(self):
        for level in (0, 1):
            self.data["levels"][level]["walls"] = [{"c0": 5, "r0": 5, "c1": 6, "r1": 5, "all": "allowed"}]
        settings = replace(self.settings, movement=replace(self.settings.movement, level_height=2.4))
        surfaces = PortalSurfaces(self.data, settings, {"allowed": True})
        for level in (0, 1, 2):
            for face in ("north", "south", "east", "west"):
                surface = PortalSurface(level, 5, 5, face)
                self.assertTrue(surfaces.status(surface).available)
                self.assertAlmostEqual(surface.frame(settings).center[1] - 1.378, level * 2.4)
        self.assertFalse(surfaces.status(PortalSurface(0, 5, 5, "north")).planned)
        self.assertTrue(surfaces.status(PortalSurface(2, 5, 5, "north")).planned)

    def test_small_surfaces_ignore_size_but_keep_face_permissions(self):
        settings = replace(self.settings, movement=replace(self.settings.movement, cell_size=0.5, wall_thickness=0.01))
        self.data["levels"][0]["floors"] = [floor(5, 5), dict(floor(5, 6), top="blocked")]
        self.data["levels"][0]["walls"] = [{"c0": 5, "r0": 5, "c1": 6, "r1": 5, "north": "allowed", "south": "blocked"}]
        surfaces = PortalSurfaces(self.data, settings, {"basement-floor": True, "allowed": True, "blocked": False})
        self.assertTrue(surfaces.status(PortalSurface(0, 5, 5)).available)
        self.assertTrue(surfaces.status(PortalSurface(0, 5, 5, turn=1)).available)
        self.assertEqual(PortalSurface(0, 5, 5).frame(settings).center, (2.75, 0, 2.75))
        self.assertTrue(surfaces.status(PortalSurface(0, 5, 5, "north")).available)
        self.assertFalse(surfaces.status(PortalSurface(0, 5, 5, "south")).available)
        self.assertTrue(surfaces.status(PortalSurface(0, 10, 10)).available)
        self.assertTrue(surfaces.status(PortalSurface(0, 10, 10, "west")).available)

    def test_wall_picking_side_boundaries_and_overlap(self):
        surfaces = PortalSurfaces(self.data, self.settings, {})
        for x, z, side in ((5.5, 4.95, "north"), (5.5, 5.05, "south"), (4.95, 5.5, "west"), (5.05, 5.5, "east")):
            self.assertEqual(surfaces.pick(0, x, z, 0.15).face, side)
        self.assertEqual(surfaces.pick(0, 5.5, 5.5, 0.15).face, "floor")
        self.assertIsNone(surfaces.pick(0, 31, 31, 0.15))
        a = PortalSurface(0, 5, 5)
        self.assertTrue(portals_overlap(a, replace(a, turn=1), self.settings))
        self.assertFalse(portals_overlap(a, replace(a, col=10), self.settings))

    def test_invalid_configuration_has_a_diagnostic(self):
        global_settings = gameplay()
        global_settings["player"]["movement_collider"]["height"] = 0
        with self.assertRaisesRegex(ValueError, "player.movement_collider.height"):
            PortalSettings.from_settings(map_settings(), "map", gameplay=global_settings, gameplay_source="gameplay")


class PortalJumpWindowTests(WindowTestCase):
    def input(self, key):
        combo = self.window.portal_jump.input_selector
        combo.setCurrentIndex(combo.findData(key))
        self.app.processEvents()

    def output(self, key):
        combo = self.window.portal_jump.output_selector
        combo.setCurrentIndex(combo.findData(key))
        self.app.processEvents()

    def start(self):
        self.window.doc.replace_with_new(insert_level_data(self.window.map_data, 1))
        self.window.set_mode(MODE_PORTAL_JUMP)
        self.input("origin")
        self.output("entry")
        self.window.select_level(1)
        self.app.processEvents()
        self.click(2, 2)
        self.window.select_level(0)
        self.app.processEvents()
        return self.window.portal_jump

    def pair(self):
        overlay = self.start()
        self.input("entry")
        self.click(3, 2)
        self.input("exit")
        self.click(5, 4)
        self.output("landings")
        return overlay

    def test_all_five_inputs_are_independent_and_do_not_edit_document(self):
        overlay = self.start()
        before = copy.deepcopy(self.window.doc.root_data)
        count, dirty = self.window.undo_stack.count(), self.window.dirty
        self.assertEqual(overlay.origin, (1, 2, 2))
        self.assertEqual(overlay.input_selector.currentData(), "origin")
        for key, col, row in (("entry_shot", 4, 2), ("entry", 3, 2), ("exit_shot", 5, 6), ("exit", 5, 4)):
            self.input(key)
            self.click(col, row)
            self.assertEqual(overlay.input_selector.currentData(), key)
            self.assertEqual(overlay.output_selector.currentData(), "entry")
        self.assertEqual(overlay.entry_shot, (0, 4, 2))
        self.assertEqual(overlay.exit_shot, (0, 5, 6))
        self.assertEqual(overlay.entry, PortalSurface(0, 3, 2, turn=3))
        self.assertEqual(overlay.exit, PortalSurface(0, 5, 4, turn=0))
        self.assertTrue(overlay.results)
        self.assertEqual(self.window.doc.root_data, before)
        self.assertEqual(self.window.undo_stack.count(), count)
        self.assertEqual(self.window.dirty, dirty)

    def test_output_selector_changes_only_the_display(self):
        overlay = self.pair()
        positions = overlay.origin, overlay.entry_shot, overlay.entry, overlay.exit_shot, overlay.exit
        results = overlay.results
        self.input("origin")
        self.assertIn("landing floor", overlay.hover_text(QPointF(5.5, 4.5)))
        self.output("entry")
        self.assertIn("Portal 1 reach", overlay.hover_text(QPointF(3.5, 2.5)))
        self.assertIn("Reachable", overlay.hover_text(QPointF(3.5, 2.5)))
        self.output("landings")
        self.assertEqual(
            positions, (overlay.origin, overlay.entry_shot, overlay.entry, overlay.exit_shot, overlay.exit)
        )
        self.assertIs(overlay.results, results)
        self.assertEqual(overlay.input_selector.currentData(), "origin")

    def test_shooting_overrides_and_fallback_reorient_only_the_matching_portal(self):
        overlay = self.pair()
        self.assertEqual(overlay.firing_origin("entry"), overlay.origin)
        self.assertEqual(overlay.firing_origin("exit"), overlay.origin)
        exit = overlay.exit
        self.input("entry_shot")
        self.click(4, 2)
        self.assertEqual(overlay.entry.turn, 3)
        self.assertEqual(overlay.exit, exit)
        self.input("exit_shot")
        self.click(5, 6)
        self.assertEqual(overlay.exit.turn, 0)
        self.input("origin")
        self.window.select_level(1)
        self.click(1, 1)
        self.assertEqual(overlay.entry_shot, (0, 4, 2))
        self.assertEqual(overlay.exit_shot, (0, 5, 6))
        self.assertEqual((overlay.entry.turn, overlay.exit.turn), (3, 0))
        self.input("entry_shot")
        overlay.use_jump_button.click()
        self.assertIsNone(overlay.entry_shot)
        self.assertEqual(overlay.entry.turn, 1)
        self.assertEqual(overlay.exit.turn, 0)
        self.assertEqual(overlay.origin, (1, 1, 1))

    def test_input_order_can_start_with_portals_and_reports_missing_jump(self):
        overlay = self.window.portal_jump
        self.window.set_mode(MODE_PORTAL_JUMP)
        self.output("landings")
        self.input("exit")
        self.click(5, 4)
        self.assertIsNotNone(overlay.exit)
        self.assertTrue(overlay.has_selection)
        self.assertIn("Set jump position", overlay.legend.text())
        self.input("entry")
        self.click(3, 2)
        self.input("exit_shot")
        self.click(5, 6)
        self.assertEqual(overlay.exit.turn, 0)
        self.input("origin")
        self.click(2, 2)
        self.assertTrue(overlay.results)
        self.assertEqual(overlay.entry.turn, 1)
        self.assertEqual(overlay.exit.turn, 0)

    def test_wall_selection_and_portal_replacement_preserve_other_inputs(self):
        overlay = self.pair()
        original_entry = overlay.entry
        size = self.window.canvas.cell_size()
        QTest.mouseClick(
            self.window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(round(5.05 * size), round(4.5 * size))
        )
        self.assertEqual(overlay.exit.face, "east")
        self.assertEqual(overlay.entry, original_entry)
        self.assertTrue(overlay.results)
        self.click(6, 6)
        self.assertEqual(overlay.exit.face, "floor")
        self.assertEqual(overlay.exit.turn, 1)
        self.input("entry")
        exit = overlay.exit
        self.click(4, 2)
        self.assertEqual(overlay.exit, exit)
        self.assertEqual(overlay.origin, (1, 2, 2))
        self.assertTrue(overlay.results)

    def test_overlap_rejected_and_cancel_preserves_selection(self):
        overlay = self.pair()
        exit = overlay.exit
        self.click(3, 2)
        self.assertEqual(overlay.exit, exit)
        canvas = self.window.canvas
        QTest.mousePress(canvas, Qt.MouseButton.LeftButton)
        QTest.keyClick(canvas, Qt.Key.Key_Escape)
        QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton)
        self.assertEqual(overlay.entry, PortalSurface(0, 3, 2, turn=1))
        self.assertEqual(overlay.exit, exit)

    def test_shooting_position_changes_preview_and_selected_floor_together(self):
        overlay = self.start()
        self.input("entry_shot")
        self.click(4, 2)
        self.input("entry")
        entry = overlay.pick(QPointF(3.5, 2.5), "entry")
        self.assertEqual(entry.turn, 3)
        self.click(3, 2)
        self.assertEqual(overlay.entry, entry)
        self.input("entry_shot")
        self.click(3, 4)
        self.assertEqual(overlay.entry.turn, 0)
        self.assertEqual(overlay.pick(QPointF(3.5, 2.5), "entry"), overlay.entry)

    def test_persistence_navigation_editing_and_clear(self):
        overlay = self.pair()
        self.input("entry_shot")
        self.click(4, 2)
        self.window.set_mode(MODE_FLOOR)
        self.app.processEvents()
        self.click(6, 6)
        self.window.undo_stack.undo()
        self.window.undo_stack.redo()
        self.window.select_level(1)
        self.window.canvas.setFocus()
        QTest.keyClick(self.window.canvas, Qt.Key.Key_Escape)
        self.assertEqual(overlay.output_selector.currentData(), "landings")
        self.assertEqual(overlay.entry_shot, (0, 4, 2))
        self.assertTrue(overlay.results)
        self.assertTrue(overlay.toolbar.isVisible())
        self.assertFalse(overlay.controls.isVisible())
        overlay.clear_action.trigger()
        self.assertFalse(overlay.has_selection)
        self.assertFalse(overlay.toolbar.isVisible())

    def test_structural_changes_clear_every_input_including_shooting_only(self):
        overlay = self.pair()
        self.input("entry_shot")
        self.click(4, 2)
        self.window.doc.apply_change("Resize", resize_map_data(self.window.map_data, 10, 10, 0, 0))
        self.assertFalse(overlay.has_selection)
        self.click(3, 2)
        self.assertIsNotNone(overlay.entry_shot)
        self.assertIsNone(overlay.origin)
        self.window.doc.apply_change("Add level", insert_level_data(self.window.map_data, 1))
        self.assertFalse(overlay.has_selection)
        self.click(3, 2)
        data = copy.deepcopy(self.window.doc.root_data)
        data["nested_geometry"] = {"platform": empty_map(8, 8)}
        self.window.doc.replace_with_new(data)
        self.assertFalse(overlay.has_selection)
        self.click(3, 2)
        self.window.doc.select_map("platform")
        self.assertFalse(overlay.has_selection)

    def test_controls_reload_and_material_invalidation_keep_inputs(self):
        overlay = self.pair()
        overlay.takeoff.setCurrentText("Step")
        self.assertFalse(overlay.margin.isEnabled())
        self.assertTrue(overlay.results)
        before = overlay.results
        overlay.movement.setCurrentText("Walk")
        self.assertIsNot(overlay.results, before)
        positions = overlay.origin, overlay.entry, overlay.exit
        settings = map_settings()
        settings["movement"]["gravity"] = None
        with patch("map_editor.portal_jump_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        self.assertEqual(overlay.results, {})
        self.assertIn("gravity", overlay.legend.text())
        self.window.reload_dependencies()
        self.assertTrue(overlay.results)
        data = copy.deepcopy(self.window.map_data)
        data["levels"][0]["floors"].append(dict(floor(3, 2), top="unknown"))
        self.window.doc.apply_change("Block entry", data)
        self.assertEqual((overlay.origin, overlay.entry, overlay.exit), positions)
        self.assertEqual(overlay.results, {})
        self.assertIn("does not allow portals", overlay.legend.text())
        self.window.undo_stack.undo()
        self.assertTrue(overlay.results)

    def test_unreachable_entry_is_retained_when_jump_position_changes(self):
        overlay = self.pair()
        settings = map_settings()
        settings["movement"]["player"]["run_speed"] = 0.1
        with patch("map_editor.portal_jump_overlay.load_map_settings", return_value=settings):
            self.window.reload_dependencies()
        positions = overlay.entry, overlay.exit
        self.assertEqual(overlay.results, {})
        self.assertIn("out of reach", overlay.legend.text())
        self.input("origin")
        self.window.select_level(1)
        self.click(3, 2)
        self.assertTrue(overlay.results)
        self.assertEqual((overlay.entry.col, overlay.entry.row), (positions[0].col, positions[0].row))
        self.assertEqual((overlay.exit.col, overlay.exit.row), (positions[1].col, positions[1].row))

    def test_preview_and_hover_clear_with_input_output_and_mode(self):
        overlay = self.start()
        self.input("entry")
        canvas = self.window.canvas
        size = canvas.cell_size()
        canvas._show_hover_label(None, QPointF(size * 3.5, size * 2.5))
        self.assertIsNotNone(overlay.preview)
        self.assertIn("Reachable", canvas._hover_label.text())
        self.input("entry_shot")
        self.assertIsNone(overlay.preview)
        self.assertFalse(canvas._hover_label.isVisible())
        self.output("landings")
        self.assertIn("Set portal 1 position", overlay.legend.text())
        self.window.set_mode(MODE_SELECT)
        self.app.processEvents()
        self.assertFalse(overlay.controls.isVisible())


if __name__ == "__main__":
    unittest.main()
