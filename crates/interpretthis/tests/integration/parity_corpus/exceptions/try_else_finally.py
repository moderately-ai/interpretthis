# Pins: try/except/else/finally ordering — else runs only when no
# except matched; finally runs unconditionally; the return inside
# try is delivered after finally.
def safe_div(a, b):
    try:
        result = a / b
    except ZeroDivisionError:
        return "div by zero"
    else:
        return result
    finally:
        pass

print(safe_div(10, 2))
print(safe_div(10, 0))
