from enum import IntEnum, IntFlag
class P(IntEnum):
    LOW = 3; HIGH = 10
print(round(P.HIGH), round(P.HIGH, 1))
print(divmod(P.HIGH, P.LOW), divmod(P.HIGH, 3))
print(pow(P.LOW, 2), pow(P.LOW, 2, 5))
print(~P.LOW, -P.LOW, +P.HIGH)
print(chr(P.HIGH + 55), P.LOW << 2, P.HIGH >> 1)
print(P.LOW & P.HIGH, P.LOW | P.HIGH, P.LOW ^ P.HIGH)
print(P.HIGH ** 2, 2 ** P.LOW)
print(str(P.HIGH), repr(P.LOW))
print(range(P.LOW)[1], [0,1,2,3,4][P.LOW])
print(sum([P.LOW, P.HIGH]), max(P.LOW, P.HIGH), min([P.LOW, P.HIGH]))
print(f"{P.HIGH:>5}", "%d" % P.HIGH, "%x" % P.HIGH)
print(bool(P.LOW), P.HIGH == 10, hash(P.HIGH) == hash(10))
