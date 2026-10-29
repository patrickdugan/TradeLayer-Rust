"""Independent arbitrary-integer test oracle; standard library only."""
from pathlib import Path
import argparse,random,hashlib
def generate():
    limit=(1<<128)-1
    cases=[(1<<127,2,2),(limit,limit,limit),(limit,limit,limit-1),(limit,1<<64,1<<64),((1<<127)+123,(1<<90)+777,(1<<100)+3),(limit-789,(1<<96)+5,(1<<120)+7)]
    rng=random.Random(148152153)
    cases += [(rng.getrandbits(128),rng.getrandbits(128),rng.getrandbits(128) or 1) for _ in range(24)]
    edges=[0,1,2,(1<<63)-1,1<<63,(1<<64)-1,1<<64,(1<<127)-1,1<<127,limit]
    cases += [(a,limit,d) for a,d in zip(edges,reversed(edges))]
    cases += [(limit,a,d) for a,d in zip(edges,edges)]
    rng=random.Random(0x153B16B00B5)
    cases += [(rng.getrandbits(128),rng.getrandbits(128),rng.getrandbits(128) or 1) for _ in range(1022)]
    assert len(cases)==1072
    lines=['# a\tb\tdivisor\tproduct_high_u128\tproduct_low_u128\tquotient_or_refusal\tremainder_or_dash']
    for a,b,d in cases:
        product=a*b;high,low=product>>128,product&limit
        if not d:q,r='zero-divisor','-'
        else:
            quotient,remainder=divmod(product,d)
            q=str(quotient) if quotient<=limit else 'overflow'
            r=str(remainder) if quotient<=limit else '-'
        lines.append(f'{a}\t{b}\t{d}\t{high}\t{low}\t{q}\t{r}')
    return ('\n'.join(lines)+'\n').encode()
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    target=Path(__file__).resolve().parents[1]/'fixtures/rewards_wide_vectors.tsv'
    raw=generate()
    if args.check:assert target.read_bytes()==raw,'independent reward vectors differ'
    else:target.write_bytes(raw)
    print(f'1072 exact vectors PASS SHA256 {hashlib.sha256(raw).hexdigest()}')
if __name__=='__main__':main()
