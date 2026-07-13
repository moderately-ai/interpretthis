# Pin: walrus assignment (`:=`) binds inside an expression and yields the value.
# Homed here because comprehensions' README calls out walrus interactions as
# the topic anchor; statement-position walrus shares the same parser path.
# Expected stdout: `5`.
if (n := 5) > 3:
    print(n)
