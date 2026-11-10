"""Differential check of stackers_core::cpu against the original func_80034C60.

    CPU_DUMP=/tmp/cpu.txt cargo test -p stackers-core cpu::tests::dump -- --ignored
    python3 tools/cpudiff.py /tmp/cpu.txt
"""
import os
import sys
sys.path.insert(0, os.path.dirname(__file__))
from mips import *
cpu=Cpu()
st={'seed':0}
def rand_hook(c):
    st['seed']=(st['seed']*0x41C64E6D+0x3039)&0xFFFFFFFF
    c.r[2]=(st['seed']>>16)&0x7FFF
cpu.hooks[0x800524A4]=rand_hook
bad=0; n=0
for line in open(sys.argv[1]):
    inp,out=line.split(' => ')
    f=inp.split()
    p,seed,A,B,blobs,pst,px,py,prot=map(int,f[:9])
    pend=[int(x) for x in f[9].rstrip(',').split(',')]
    params=list(map(int,f[10:16]))
    mode,delay,wig,pad,tx,ty,trot=map(int,f[16:23])
    cells=f[23].rstrip(',').split(',')
    i=0
    for c in range(8):
        for r_ in range(19):
            s,col,x,y=map(int,cells[i].split(':')); i+=1
            a=cell(p,c,r_)
            cpu.seth(a,s); cpu.seth(a+2,col); cpu.setw(a+0xC,x); cpu.setw(a+0x10,y)
    cpu.setw(0x800C2FB8,1-p)
    P=0x80100250+0x50*p
    cpu.setw(P,px); cpu.setw(P+4,py); cpu.seth(P+0xC,pst); cpu.seth(P+0x12,prot)
    cpu.seth(0x80069A08+0x90*p,A); cpu.seth(0x800699F0+0x90*p,B)
    for k,v in enumerate(pend): cpu.seth(0x801006A8+14*p+2*k,v)
    for k,v in enumerate(params): cpu.seth(0x800B13E8+12*p+2*k,v)
    cpu.seth(0x801005FA+4*p,0); cpu.seth(0x80100634+2*p,blobs); cpu.seth(0x80100638+2*p,0)
    cpu.seth(0x800B1428+2*p,mode); cpu.seth(0x801005E8+2*p,delay); cpu.seth(0x8010ADA8+2*p,wig)
    cpu.seth(0x800C2F58+2*p,pad)
    T=0x800C2F10+12*p
    cpu.setw(T,tx); cpu.setw(T+4,ty); cpu.seth(T+8,trot)
    st['seed']=seed
    cpu.call(0x80034C60)
    got=[cpu.rd(0x800C2F58+2*p,2), cpu.h(0x800B1428+2*p), cpu.h(0x801005E8+2*p), cpu.h(0x8010ADA8+2*p),
         s32(cpu.w(T)), s32(cpu.w(T+4)), cpu.rd(T+8,2), st['seed']]
    o=out.split()
    want=list(map(int,o[:8]))
    rl=o[8] if len(o)>8 else ''
    el=''
    for L in [0x80066608,0x80066940,0x80066480,0x80066BB0,0x800667B8,0x800662F8]:
        b=L+0xC4*p
        for k in range(cpu.h(b)):
            e=b+4+12*k
            el+='%d:%d:%d,'%(s32(cpu.w(e)),s32(cpu.w(e+4)),cpu.rd(e+8,2))
        el+='|'
    n+=1
    if got!=want:
        bad+=1
        if bad<=3: print('line',n,'p',p,'A',A,'B',B,'\n  emu ',got,el,'\n  rust',want,rl)
print(n,'cases',bad,'mismatches')
