# Pins: `sorted(d, key=d.get)` — sort dict keys by their values via bound
# method. Customer-listed pattern. `sorted` historically ignored `key=`
# entirely — this pins that it must thread through like min/max do.
d = {'A': 3, 'B': 1, 'C': 2}
print(sorted(d, key=d.get))
