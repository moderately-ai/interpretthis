# Pins: str.partition / .rpartition — return a (head, sep, tail)
# triple; when no separator found, the tail (partition) or head
# (rpartition) is empty.
print("a,b,c".partition(","))
print("no-sep".partition(","))
print("a-b-c".rpartition("-"))
