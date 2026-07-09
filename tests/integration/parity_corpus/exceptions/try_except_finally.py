# Pins: try / except / finally execution order on a raise inside try.
result = []
try:
    result.append("try")
    raise ValueError("oops")
except ValueError:
    result.append("except")
finally:
    result.append("finally")
print(",".join(result))
