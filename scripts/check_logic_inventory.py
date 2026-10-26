"""Check the pinned per-type review against real source dispatch and mapper scope."""
from pathlib import Path
import hashlib
import json
import re


def main():
    root = Path(__file__).resolve().parent.parent
    data = json.loads((root / 'docs/LOGIC_BY_TX_TYPE.json').read_text(encoding='utf-8'))
    assert data['schema'] == 'tl-logic-reference-review-v1'
    assert data['jsCommit'] == 'dca0888b827c5bc28fa9189bc9e1d507f5c5670c'
    assert data['evidenceScope'] == 'static per-type inventory; executed property and clearlist checks are recorded separately'
    for pin in data['sourcePins'] + data['runtimePins']:
        raw = (root / pin['path']).read_bytes()
        if pin.get('lfNormalized'):
            raw = raw.replace(b'\r\n', b'\n')
        assert hashlib.sha256(raw).hexdigest() == pin['sha256'], 'Review needs refresh: ' + pin['path']
    source = (root / 'fixtures/native_genesis/source/logic.js').read_text(encoding='utf-8')
    switch = source.split('async typeSwitch(txNumber, params)', 1)[1].split('async activateTradeLayer', 1)[0]
    routes = {}
    for match in re.finditer(r'case\s+(\d+)\s*:(.*?)(?:break;)', switch, flags=re.S):
        handler = re.search(r'Logic\.(\w+)\s*\(', match[2])
        assert handler is not None
        routes[int(match[1])] = handler[1]
    assert set(routes) == set(range(36))
    mapper = (root / 'src/types_economic.rs').read_text(encoding='utf-8')
    match = re.search(r'if !matches!\(ty,\s*([0-9 |]+)\)', mapper)
    assert match is not None
    mapped = {int(n.strip()) for n in match[1].split('|')}
    rows = data['rows']
    assert data['catalogExecutionTypes'] == [1, 7, 8, 9, 11, 12]
    assert [row['type'] for row in rows] == list(range(37))
    fields = ['sourceRoute', 'sourceEffects', 'sourceDefects', 'runtimeScope', 'missingLogic',
              'nextTask', 'evidence', 'preStateWitness', 'postStateWitness', 'inputWitness',
              'contextWitness', 'dependencyWitness', 'proofCoverage', 'fidelity']
    for row in rows:
        number = row['type']
        assert all(isinstance(row[key], str) and row[key].strip() for key in fields)
        assert row['sourceRouted'] == (number in routes)
        assert row['sourceHandler'] == routes.get(number)
        assert row['mapperCandidate'] == (number in mapped)
        assert row['catalogExecutionCandidate'] == (number in data['catalogExecutionTypes'])
        assert row['fullReferenceParity'] is False
        assert row['transitionProofComplete'] is False
    assert data['transitionWitness'] == 'not-exported'
    print(f'PASS: 36 exact source routes + unrouted36; {len(mapped)} mapper candidates; parity/proof claims remain explicitly incomplete')


if __name__ == '__main__':
    main()
