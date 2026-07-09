# Pins: for/while loops support an `else` clause that runs when
# the loop completes normally (no break). break skips it.
for x in range(3):
    print(x)
else:
    print("done")

i = 0
while i < 3:
    i += 1
else:
    print("while-else")

for x in range(5):
    if x == 2:
        break
else:
    print("never")
print("after-break")
