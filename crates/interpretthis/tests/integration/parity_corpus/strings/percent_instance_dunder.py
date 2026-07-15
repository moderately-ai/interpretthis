# %-formatting coerces user-class instances through the same dunders CPython uses.
class Idx:
    def __index__(self): return 42
class Num:
    def __int__(self): return 7
class Flt:
    def __float__(self): return 3.5
class Txt:
    def __str__(self): return "STR"
    def __repr__(self): return "REPR<>"
class Both:
    def __index__(self): return 9
    def __str__(self): return "NINE"

print("%d" % Idx(), "%d" % Num())
print("%i %u" % (Num(), Idx()))
print("%x %X %o" % (Idx(), Idx(), Idx()))
print("%c" % Idx())
print("%5d|%-5d|%05d" % (Idx(), Idx(), Num()))
print("%f %.2f %e %g" % (Flt(), Flt(), Flt(), Flt()))
print("%s %r %a" % (Txt(), Txt(), Txt()))
print("val=%s end" % Txt())
print("%10s|%-10s" % (Txt(), Txt()))
print("%(k)d and %(k)s" % {"k": Both()})
print("mixed %s=%d" % (Txt(), Idx()))
