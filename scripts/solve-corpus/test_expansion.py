"""Selection, censored runtime estimates, and resuming indexed corpora."""

import contextlib
import io
import json
from pathlib import Path
import sqlite3
import tempfile
from types import SimpleNamespace
import unittest

import estimate
from expand import area, fingerprint, inventory, selection, size_band
from run import Batch


class ExpansionTests(unittest.TestCase):
    def test_inventory_includes_new_games_and_array_levels(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            one = root / 'apps/new-game/src/game/levelpacks/mus4/level-1.json'
            one.parent.mkdir(parents=True)
            one.write_text(json.dumps({'width': 4, 'height': 5}))
            many = root / 'apps/combination/public/assets/levels/Simple.json'
            many.parent.mkdir(parents=True)
            many.write_text(json.dumps([{'grid': 'ab|cd'}, {'grid': [[1, 2, 3]]}]))
            records = inventory(root)
            self.assertEqual(len(records), 3)
            self.assertEqual({r['board_area'] for r in records}, {3, 4, 20})
            new = next(r for r in records if r['game'] == 'new-game')
            self.assertEqual(new['source_tier'], 4)
            self.assertEqual(new['variant'], 'main')
            self.assertEqual(area({}, 'other'), None)
            self.assertEqual(size_band(None), 'unknown')

    def test_selection_excludes_contents_preserves_strata_and_is_reproducible(self):
        items = [{'id': f'{variant}-{band}-{i}', 'game': 'g', 'variant': variant,
                  'size_band': band, 'source_pack': f'pack-{i % 2}', 'board_area': a,
                  'content_sha256': f'{variant}-{band}-{i}'}
                 for variant in ('a', 'b') for band, a in [('up-to-20', 16), ('over-36', 64)]
                 for i in range(8)]
        excluded = {'a-up-to-20-0'}
        selected = selection(items, excluded, 6, 123)
        self.assertEqual(len(selected), 6)
        self.assertEqual(len({(r['variant'], r['size_band']) for r in selected}), 4)
        self.assertNotIn('a-up-to-20-0', [r['id'] for r in selected])
        self.assertEqual(selected, selection(list(reversed(items)), excluded, 6, 123))
        self.assertTrue(all(r['board_area'] <= 36 for r in selection(items, excluded, 6, 123, 36)))
        self.assertEqual(len(selection(items, excluded, 1, 123)), 4)

    def test_estimate_keeps_censoring_and_unknown_population_separate(self):
        population = {'entries': [
            {'game': 'a', 'board_area': 16, 'content_sha256': f'a{i}'} for i in range(10)] + [
            {'game': 'b', 'board_area': 64, 'content_sha256': f'b{i}'} for i in range(10)]}
        samples = {'a0': {'capped_seconds': 2, 'complete_within_budget': True, 'unresolved_at_budget': False},
                   'a1': {'capped_seconds': 60, 'complete_within_budget': False, 'unresolved_at_budget': True}}
        p = estimate.project(population, samples, 60, 2)
        self.assertAlmostEqual(p['projected_solver_hours_measured_strata'], 10 * 31 / 7200)
        self.assertAlmostEqual(p['projected_solver_hours_if_unmeasured_use_full_budget'], (310 + 600) / 7200)
        self.assertEqual(p['unmeasured_population'], 10)
        self.assertEqual(p['projected_unfinished_measured_strata'], 5)

    def test_short_successful_resume_does_not_hide_slow_initial_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = {'width': 4, 'height': 4}
            (root / 'input.json').write_text(json.dumps(data))
            (root / 'manifest.json').write_text(json.dumps({'instances': [{'id': 'p', 'input': 'input.json'}]}))
            with sqlite3.connect(root / 'corpus.sqlite') as db:
                db.execute('CREATE TABLE attempts(puzzle_id,phase,status,started_utc,settings_json,seconds)')
                db.execute('INSERT INTO attempts VALUES (?,?,?,?,?,?)',
                           ('p', 'baseline', 'checkpoint', '1', json.dumps({'command': ['solver', 'build'], 'budget_seconds': 180}), 181))
                db.execute('INSERT INTO attempts VALUES (?,?,?,?,?,?)',
                           ('p', 'baseline', 'complete', '2', json.dumps({'command': ['solver', 'resume'], 'budget_seconds': 600}), 2))
            samples = estimate.observations([root], {fingerprint(data)}, 60)
            self.assertEqual(len(samples), 1)
            self.assertEqual(samples[fingerprint(data)]['capped_seconds'], 60)
            self.assertFalse(samples[fingerprint(data)]['complete_within_budget'])
            self.assertEqual(estimate.observations([root], {'different-content'}, 60), {})

    def test_resuming_run_preserves_analysis_foreign_keys(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'manifest.json').write_text(json.dumps({
                'inventory': {'g': {}}, 'instances': [{'id': 'p', 'game': 'g', 'variant': 'main',
                'source_pack': 'pack', 'source_tier': 1, 'board_area': 16, 'input': 'p.json', 'input_sha256': 'input'}]}))
            batch = Batch(SimpleNamespace(corpus=root, refine=None, ids=None))
            with contextlib.redirect_stdout(io.StringIO()):
                batch.record('p', {'status': 'checkpoint', 'started_utc': '1', 'archive_sha256': 'old'})
            with batch.db() as db:
                db.execute('CREATE TABLE analysis(puzzle_id,phase,archive_hash, '
                           'FOREIGN KEY(puzzle_id,phase) REFERENCES runs(puzzle_id,phase))')
                db.execute("INSERT INTO analysis VALUES ('p','baseline','old')")
            with contextlib.redirect_stdout(io.StringIO()):
                batch.record('p', {'status': 'running', 'started_utc': '2'})
                batch.record('p', {'status': 'complete', 'started_utc': '2', 'archive_sha256': 'new'})
            with batch.db() as db:
                self.assertEqual(db.execute('SELECT archive_hash FROM analysis').fetchone()[0], 'old')
                self.assertEqual(db.execute('SELECT archive_sha256 FROM runs').fetchone()[0], 'new')
                self.assertEqual(db.execute('SELECT count(*) FROM attempts').fetchone()[0], 2)
                self.assertEqual(db.execute('PRAGMA foreign_key_check').fetchall(), [])


if __name__ == '__main__':
    unittest.main()
