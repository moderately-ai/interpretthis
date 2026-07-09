# Pins: finally always runs even when no exception fires.
result = []
try:
    result.append("try")
finally:
    result.append("finally")
print(",".join(result))
