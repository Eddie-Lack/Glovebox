"""Part of the desktop app's --self-test: assembles, disassembles and emulates a MIPS pointer load with Keystone,
Capstone and Unicorn, as libgarib's ROM tool does when it dumps or patches a ROM. It runs in a child process, as the
tool does. Each step prints a line first, so a crash shows where it happened."""
say = lambda s: print(s, flush=True)
say("step: import")
from libgarib.rom import ptrCode  # noqa: E402

say("step: assemble (keystone)")
code = ptrCode.get_code(0x80123456, "T0")
say("step: disassemble (capstone)")
ptrCode.get_output_reg(code)
say("step: emulate (unicorn)")
value = ptrCode.emulate(code)
say("engines: %s" % ("ok" if value == 0x80123456 else "wrong value 0x%x" % value))
