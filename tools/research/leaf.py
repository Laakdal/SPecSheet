"""Linear disassembly of eurotrucks2.exe from an address to the first `ret`.

For leaf functions, which have no .pdata entry, so disasm.py cannot find
their extent. Needs `pip install capstone`. Read-only; never commit the
listing (it is SCS code, D13). Record observations instead.
Usage: python -I leaf.py <exe> <va> [max instructions, default 80]
"""
import struct,sys,capstone
d=open(sys.argv[1],'rb').read(); a=int(sys.argv[2],16); mx=int(sys.argv[3]) if len(sys.argv)>3 else 80
pe=struct.unpack_from("<I",d,0x3C)[0]; n=struct.unpack_from("<H",d,pe+6)[0]; opt=struct.unpack_from("<H",d,pe+20)[0]
secs=[]
for i in range(n):
    o=pe+24+opt+i*40; vs,va,rs,ra=struct.unpack_from("<IIII",d,o+8); secs.append((va,rs,ra))
def off(rva):
    for va,rs,ra in secs:
        if va<=rva<va+rs: return ra+rva-va
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
o=off(a-0x140000000)
for k,i in enumerate(md.disasm(d[o:o+4000],a)):
    note=""
    if 'rip +' in i.op_str or 'rip -' in i.op_str:
        try:
            disp=int(i.op_str.split('rip ')[1].split(']')[0].replace(' ',''),16)
            t=i.address+i.size+disp; to=off(t-0x140000000)
            note=f"  ; -> {t:#x} f32 {struct.unpack_from('<f',d,to)[0]:g}"
        except Exception: pass
    print(f"{i.address:x}: {i.mnemonic} {i.op_str}{note}")
    if i.mnemonic=='ret' or k>mx: break
