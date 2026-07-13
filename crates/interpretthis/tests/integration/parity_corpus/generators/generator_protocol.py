# Pins: generator iterator protocol next/send/throw/close on eager buffers.
def count(n):
    i = 0
    while i < n:
        yield i
        i += 1

g = count(3)
print(next(g))
print(g.send(None))
print(g.__next__())
try:
    next(g)
except StopIteration:
    print("stop")

g2 = count(2)
print(next(g2))
g2.close()
try:
    next(g2)
except StopIteration:
    print("closed")

g3 = count(5)
print(next(g3))
try:
    g3.throw(ValueError, "boom")
except ValueError as e:
    print(e)
try:
    next(g3)
except StopIteration:
    print("thrown-exhausted")

# first send must be None
g4 = count(1)
try:
    g4.send(1)
except TypeError:
    print("send-first")
