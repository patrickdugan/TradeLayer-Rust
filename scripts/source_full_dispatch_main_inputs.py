"""Project genuine executed Main corpus INPUTS/outcomes, never poststate seeds."""
import argparse
import hashlib
import json
import os
from pathlib import Path

HERE = Path(__file__).resolve().parent
STAGE = HERE.parent if HERE.name == 'scripts' else HERE
ROOT = Path(os.environ.get('PROOF_REPO', STAGE if HERE.name == 'scripts' else HERE.parent.parent / 'outputs' / 'TradeLayer-Rust'))
U = ['undefined']

def get(v, key):
    return next((x for k, x in v[1] if k == key), U) if v[0] == 'object' else U

def txt(v):
    assert v[0] == 'string', v
    return v[1]

def obj(**values):
    return ['object', list(values.items())]

def norm(v):
    t = v[0]
    if t == 'object':
        return ['object', [[k, norm(x)] for k, x in v[1]]]
    if t in ('array', 'set'):
        return ['array', [norm(x) for x in v[1]]]
    if t == 'map':
        return ['array', [['array', [norm(k), norm(x)]] for k, x in v[1]]]
    if t == 'regexp':
        pattern, flags = v[1:]
        return ['string', '^tx'] if pattern == '^tx' and flags == '' else obj(**{'$regexp': ['string', pattern], '$flags': ['string', flags]})
    assert t not in ('function', 'cycle'), v
    return v

def invoke(target, args):
    return obj(kind=['string', 'Invoke'], target=['string', target], arguments=['array', args])

def kinds(target, arguments):
    if target not in ('Main.processTxSet', 'Main.processTx', 'Main.blockHandlerMid'):
        return []
    if not arguments or arguments[0][0] != 'array':
        return []
    found = set()
    for row in arguments[0][1]:
        value = get(row, 'value') if target == 'Main.processTxSet' else row
        payload = get(value, 'payload')
        if payload[0] == 'string' and payload[1]:
            found.add(int(payload[1][0], 36))
    return sorted(found)

def project():
    output, inventory, selected = [], [], set()
    extra_witnesses = {'first10/05-indexed-real-bootstrap', 'first10/06-indexed-bootstrap-repeat', 'amm/02-indexed-contract-add-real-tally', 'amm/03-indexed-contract-redeem-real-tally', 'spot/03-live-noop-cancel-triggers-vanilla-match-failure', 'spot/04-processed-cancel-retry-only-withdraws'}
    files = ['main_phase_vectors.jsonl', 'main_lower_vectors.jsonl', 'main_upper_vectors.jsonl', 'main_spot_vectors_receivers_v2.jsonl', 'main_activation_vectors.jsonl', 'main_amm_vectors.jsonl']
    allowed = {'Main.processTxSet', 'Main.processTx', 'Main.blockHandlerMid', 'Main.loadMaxProcessedHeight', 'Main.saveMaxProcessedHeight', 'Main.loadTrackHeight', 'Main.saveTrackHeight'}
    for file in files:
        data = (ROOT / 'fixtures' / file).read_bytes()
        rows = [json.loads(x) for x in data.decode().splitlines()]
        inventory.append({'file': file, 'sha256': hashlib.sha256(data).hexdigest(), 'sourceFrames': len(rows)})
        tainted = True
        for row in rows:
            command = norm(row['command'])
            reset = get(command, 'reset')
            name = row['name']
            if reset != U:
                tainted = False
                options = get(reset, 'options')
                inputs = []
                entry = get(options, 'entry')
                if entry != U:
                    inputs.append(obj(kind=['string', 'SetEntry'], entry=entry))
                if file == 'main_activation_vectors.jsonl':
                    profile = get(options, 'clientProfile')
                    profile_arg = ['null'] if profile == U else obj(chain=get(profile, 'chain'), test=get(profile, 'test'))
                    inputs.append(invoke('SourceActivation.configureClient', [profile_arg]))
                    height = get(profile, 'height')
                    if height != U:
                        inputs.append(obj(kind=['string', 'SetHeight'], height=height))
                # Only reset constructor tables are explicit external input.
                # All subsequent financial state must come from actual commands.
                tables = norm(get(get(row['state'], 'source'), 'tables'))
                assert tables[0] == 'array', name
                for table in tables[1]:
                    inputs.append(obj(kind=['string', 'Import'], collection=get(table, 'name'), rows=get(table, 'rows')))
                for key, target in [('environment', 'SourceEnvironment.configure'), ('artifacts', 'SourceArtifacts.configure')]:
                    value = get(options, key)
                    if value != U:
                        inputs.append(invoke(target, [value]))
                if get(options, 'bindRpc') == ['boolean', True]:
                    target = 'SourceGlobal.bindSpotRpcDocuments' if file in ('main_upper_vectors.jsonl', 'main_spot_vectors_receivers_v2.jsonl') else 'SourceGlobal.bindRpcTransactions'
                    inputs.append(invoke(target, []))
                if get(options, 'bindTally') == ['boolean', True]:
                    inputs.append(invoke('SourceGlobal.bindTally', []))
                if get(options, 'propertyRegistry') == ['boolean', True]:
                    inputs.append(invoke('SourceGlobal.bindPropertyRegistry', []))
                if get(options, 'propertyPool') == ['boolean', True]:
                    number = lambda x: ['number', x]
                    inputs.append(invoke('SourceAMM.construct', [number('0000000000000000'), number('4059000000000000'), number('4024000000000000'), number('0000000000000000')]))
                    inputs.append(invoke('SourceAMM.attachPropertyPool', [number('3ff0000000000000'), number('401c000000000000'), obj(sourceAmmInstance=number('3ff0000000000000'))]))
                init = get(options, 'init')
                if file == 'main_activation_vectors.jsonl' and init != U:
                    inputs.append(invoke('Activation.init', [init]))
                output.append(obj(name=['string', name], file=['string', file], reset=['boolean', True], inputs=['array', inputs]))
                continue
            if get(command, 'profile') != U:
                continue
            if tainted:
                continue
            explicit = get(command, 'import')
            if explicit != U:
                output.append(obj(name=['string', name], file=['string', file], input=obj(kind=['string', 'Import'], collection=get(explicit, 'collection'), rows=get(explicit, 'rows'))))
                continue
            if get(command, 'binding') == ['string', 'rpcTransactions']:
                output.append(obj(name=['string', name], file=['string', file], input=invoke('SourceGlobal.bindRpcTransactions', [])))
                continue
            target = get(command, 'target')
            if target == U:
                continue
            target = txt(target)
            if row.get('domain') == 'unavailable-capability' or (target.startswith('Main.') and target not in allowed):
                output.append(obj(name=['string', name], file=['string', file], unavailable=['boolean', True]))
                tainted = True
                continue
            args = get(command, 'args')[1]
            routed = kinds(target, args)
            new = sorted(set(routed) - selected)
            selected.update(new)
            failure = get(command, 'failure')
            output.append(obj(name=['string', name], file=['string', file], target=['string', target], args=['array', args], failure=failure, kinds=['array', [['string', str(x)] for x in routed]], select=['array', [['string', str(x)] for x in new]], witness=['boolean', bool(new) or name in extra_witnesses], expected=norm(row['result'])))
    assert selected == set(range(36)), selected
    return output, inventory

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    rows, inventory = project()
    raw = ('\n'.join(json.dumps(x, ensure_ascii=False, separators=(',', ':')) for x in rows) + '\n').encode()
    meta = {'sourceCommit': 'dca0888b827c5bc28fa9189bc9e1d507f5c5670c', 'rule': 'constructor reset tables only; actual command history thereafter; unavailable prefixes isolated until reset', 'sources': inventory, 'frames': len(rows), 'actualMainCalls': sum(get(r, 'target')[0] == 'string' and get(r, 'target')[1].startswith('Main.') for r in rows), 'actualSupportCalls': sum(get(r, 'target')[0] == 'string' and not get(r, 'target')[1].startswith('Main.') for r in rows), 'witnessCases': sum(get(r, 'witness') == ['boolean', True] for r in rows), 'isolatedUnavailableFrames': sum(get(r, 'unavailable') == ['boolean', True] for r in rows), 'routedTypes': list(range(36)), 'sha256': hashlib.sha256(raw).hexdigest()}
    files = {'fixtures/source_full_dispatch_main_inputs.jsonl': raw, 'fixtures/source_full_dispatch_main_inventory.json': (json.dumps(meta, indent=2) + '\n').encode()}
    for name, data in files.items():
        p = STAGE / name
        if args.check:
            assert p.read_bytes() == data, name
        else:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
    print(json.dumps(meta))

if __name__ == '__main__':
    main()
