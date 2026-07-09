# Pins: re-raising from inside an inner except propagates to the outer except.
result = "none"
try:
    try:
        raise ValueError("inner")
    except ValueError:
        result = "inner_caught"
        raise TypeError("outer")
except TypeError:
    result = result + ",outer_caught"
print(result)
