import io
import contextlib

buf = io.StringIO()
buf.write("hello")
buf.write(" world")
print(buf.getvalue(), buf.tell())
buf.seek(0)
print(buf.read(), buf.tell())
buf.seek(0)
print(buf.read(5))

lines = io.StringIO("line1\nline2\nline3")
print(lines.readline().strip(), lines.readline().strip())
print(lines.readlines())

w = io.StringIO()
w.writelines(["a\n", "b\n", "c\n"])
print(repr(w.getvalue()))

print(io.StringIO("seed").getvalue())
print(type(io.StringIO()).__name__)
print(bool(io.StringIO()), bool(io.StringIO("x")))

# redirect_stdout captures print output.
out = io.StringIO()
with contextlib.redirect_stdout(out):
    print("captured 1")
    print("captured 2")
print("normal")
print("cap:", repr(out.getvalue()))

# Nested redirect and loop output.
buf2 = io.StringIO()
with contextlib.redirect_stdout(buf2):
    for i in range(3):
        print(f"n={i}")
print(buf2.getvalue().count("\n"), repr(buf2.getvalue()))

# StringIO as its own context manager.
with io.StringIO() as f:
    f.write("ctx")
    print(f.getvalue())

# truncate and seek.
t = io.StringIO("abcdefgh")
t.seek(3)
print(t.tell(), t.read())
t.seek(3)
t.truncate()
print(t.getvalue())

# Reference semantics: alias shares the buffer.
a = io.StringIO()
b = a
b.write("shared")
print(a.getvalue())
