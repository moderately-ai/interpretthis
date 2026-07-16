import calendar
print(calendar.isleap(2024), calendar.isleap(2023), calendar.isleap(1900), calendar.isleap(2000))
print(calendar.leapdays(2000, 2024))
print(calendar.weekday(2024, 3, 15))
wd, days = calendar.monthrange(2024, 2)
print(int(wd), days)
print(calendar.month_name[3])
print(calendar.day_name[0])
print(calendar.month_abbr[12])
print(calendar.day_abbr[6])
print(list(calendar.month_name)[1:4])
import math
print(calendar.mdays)
