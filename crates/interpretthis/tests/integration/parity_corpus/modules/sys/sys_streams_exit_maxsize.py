import sys
print("normal")
print("to stderr", file=sys.stderr)
print("after stderr")
print(sys.maxsize)
print(sys.maxsize == 2**63 - 1)
print(sys.byteorder)
import io
buf = io.StringIO()
print("a", "b", file=buf, sep="+")
print("c", file=buf)
print(repr(buf.getvalue()))
print("x", "y", "z", file=sys.stdout, sep="-")
try:
    sys.exit(0)
except SystemExit as e:
    print("caught exit", e.code)
try:
    sys.exit()
except SystemExit as e:
    print("caught bare exit", repr(e.code))
try:
    sys.exit("error message")
except SystemExit as e:
    print("caught msg exit", e.code)
print("survived all exits")
def log(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)
log("debug info", "here")
print("visible")
