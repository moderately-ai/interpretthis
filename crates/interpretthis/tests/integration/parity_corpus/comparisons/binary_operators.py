# Pins: <, >, ==, !=, <=, >= on integer pairs.
results = []
results.append(str(1 < 2))
results.append(str(2 > 1))
results.append(str(1 == 1))
results.append(str(1 != 2))
results.append(str(1 <= 1))
results.append(str(2 >= 3))
print(",".join(results))
