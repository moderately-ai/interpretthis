# CPython's ZeroDivisionError message is operator- and type-specific, and float
# ** raises OverflowError (not inf) when a finite base/exponent overflows.
def zde(f):
    try:
        f()
    except ZeroDivisionError as e:
        return str(e)
print(zde(lambda: 10 % 0))
print(zde(lambda: 10.5 % 0))
print(zde(lambda: (2 ** 70) % 0))
print(zde(lambda: 10 // 0))
print(zde(lambda: 10.0 // 0))
print(zde(lambda: (2 ** 70) // 0))
print(zde(lambda: 10 / 0))
print(zde(lambda: 10.0 / 0))
print(zde(lambda: 10 / 0.0))
print(zde(lambda: divmod(10, 0)))
print(zde(lambda: 5.0 % 0.0))

def ovf(f):
    try:
        f()
    except OverflowError as e:
        return str(e)
print(ovf(lambda: 1.5e300 ** 2))
print(ovf(lambda: 1e200 ** 5))
# no overflow / inf inputs do not raise
print(1e308 * 10)
print(float("inf") ** 2)
print(2.0 ** 10, 10.0 ** 2, 0.5 ** 1000)
