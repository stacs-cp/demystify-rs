"""Cross-corpus coverage, equal-puzzle summaries, route weighting, and restart semantics."""

import contextlib
import io
import json
from pathlib import Path
import sqlite3
import tempfile
from types import SimpleNamespace
import unittest

import analyse as driver
from expand import fingerprint
import overall
from run import Batch
from story import analyse, VERSION
from sweep import interleave, remaining
from test_story import fixture


class OverallTests(unittest.TestCase):
    def test_quantiles_keep_missing_separate(self):
        self.assertEqual(overall.distribution([None, None]), {'n': 0})
        d = overall.distribution([None, 0, 10])
        self.assertEqual(d['n'], 2)
        self.assertEqual(d['median'], 5)
        self.assertEqual(d['p10'], 1)
        self.assertEqual(d['p90'], 9)

    def test_route_weighting_does_not_count_dag_states_as_moves(self):
        result = analyse(fixture(3, lambda known, lit: 1))
        with sqlite3.connect(':memory:') as db:
            db.row_factory = sqlite3.Row
            db.executescript(driver.SCHEMA)
            driver.index(db, {'puzzle_id': 'p', 'phase': 'baseline'},
                         {'version': VERSION, 'status': 'complete'}, result)
            row = db.execute('SELECT * FROM story_runs').fetchone()
            values = overall.features(db, row)
        self.assertAlmostEqual(values['moves_expected'], 3)
        # Eight DAG states, but every route has just one forced final move.
        self.assertAlmostEqual(values['choke_step_fraction'], 1 / 3)
        self.assertAlmostEqual(values['independent_step_fraction'], 2 / 3)
        self.assertEqual(values['chain_max'], 1)
        self.assertEqual(values['peak_hard_gap'], 0)
        self.assertIsNone(values['cheapest_variable_fraction'])

    def test_equal_puzzle_weight_and_metric_denominators(self):
        entries = [{'game': 'g', 'size_band': 'small', 'source_pack': 'pack', 'source_tier': 1,
                    'status': status, 'metrics': metrics}
                   for status, metrics in [('complete', {'moves_expected': 2, 'chain_max': None}),
                                           ('complete', {'moves_expected': 100, 'chain_max': 4}),
                                           ('checkpoint', {})]]
        group = next(g for g in overall.summarise(entries) if g['dimension'] == 'game')
        self.assertEqual(group['population'], 3)
        self.assertEqual(group['analysed'], 2)
        self.assertEqual(group['metrics']['moves_expected']['median'], 51)
        self.assertEqual(group['metrics']['chain_max']['n'], 1)

    def test_dedup_current_contents_and_null_pending_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = {'width': 3, 'height': 1}
            item = {'id': 'p', 'game': 'g', 'variant': 'main', 'source_pack': 'pack', 'source_tier': 1,
                    'board_area': 3, 'input': 'input.json', 'input_sha256': 'input', 'content_sha256': fingerprint(data)}
            (root / 'input.json').write_text(json.dumps(data))
            (root / 'manifest.json').write_text(json.dumps({'inventory': {'g': {}}, 'instances': [item]}))
            batch = Batch(SimpleNamespace(corpus=root, refine=None, ids=None))
            with contextlib.redirect_stdout(io.StringIO()):
                batch.record('p', {'status': 'complete', 'archive_sha256': 'graph', 'started_utc': '1'})
            with batch.db() as db:
                db.executescript(driver.SCHEMA)
                driver.index(db, {'puzzle_id': 'p', 'phase': 'baseline'},
                             {'version': VERSION, 'status': 'complete', 'archive_sha256': 'graph'},
                             analyse(fixture(3, lambda known, lit: 1)))
            inventory = {'entries': [item, {**item, 'id': 'changed', 'content_sha256': fingerprint({'width': 4})}]}
            with contextlib.redirect_stdout(io.StringIO()):
                result = overall.write(inventory, [root, root], root / 'out')
            self.assertEqual([r['status'] for r in result['puzzles']], ['complete', 'pending'])
            with sqlite3.connect(root / 'out/overall.sqlite') as db:
                self.assertEqual(db.execute("SELECT moves_expected FROM puzzles WHERE id='changed'").fetchone()[0], None)
                self.assertEqual(db.execute('SELECT count(*) FROM puzzles').fetchone()[0], 2)
            # An old analysis must stop contributing as soon as its graph changes.
            with batch.db() as db:
                db.execute("UPDATE runs SET archive_sha256='new-graph'")
            self.assertEqual(overall.collect(inventory, [root])[0]['metrics'], {})

    def test_restart_retains_timeouts_and_recovers_interrupted_jobs(self):
        items = [{'id': str(i), 'game': 'a' if i < 4 else 'b'} for i in range(8)]
        ordered = interleave(items, 42)
        self.assertEqual(ordered, interleave(list(reversed(items)), 42))
        self.assertEqual([i['game'] for i in ordered], ['a', 'b'] * 4)
        pending = remaining(items, [{'puzzle_id': '0', 'status': 'checkpoint'},
                                    {'puzzle_id': '1', 'status': 'complete'},
                                    {'puzzle_id': '2', 'status': 'running'},
                                    {'puzzle_id': '3', 'status': 'failed'}])
        self.assertEqual([i['id'] for i in pending], ['2', '4', '5', '6', '7'])


if __name__ == '__main__':
    unittest.main()
