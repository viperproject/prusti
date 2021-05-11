#!/usr/bin/env python3

# %%
import json
from statistics import mean


# %%
mainfile = open('benchmark-vanilla.json')
cefile = open('benchmark-ce.json')
main = json.load(mainfile)
ce = json.load(cefile)


# %%
averages = []
for k,v in main.items():
    if k in ce:
        item2 = ce[k]
        avg1 = mean(v)
        avg2 = mean(item2)
        averages.append((avg1, avg2)) 
# %%
differences = [y - x for (x,y) in averages]
average_overall = mean(differences)
len(differences)

# %%
percentage_decrease = [(y - x)/ y * 100 for (x,y) in averages]
mean(percentage_decrease)
count = len([x for x in percentage_decrease if x <= 20])
count/121
# %%
import matplotlib.pyplot as plt
plt.hist(differences, bins = 200)
plt.gca().set(xlabel='increase verification-time (seconds)');
plt.gca().set(ylabel='number of testcases')
# %%

plt.hist(percentage_decrease, bins=100)
plt.gca().set(xlabel='increase verification-time (%)')
plt.gca().set(ylabel='number of testcases')
# %%
import csv

# %%
with open('benchmark.csv', 'w', newline='') as file:
    writer = csv.writer(file)
    writer.writerow(["filename", "time", "time-ce", "difference", "difference_percentage"])
    for k,v in main.items():
        if k in ce:
            item_ce = ce[k]
            avg1 = mean(v)
            avg2 = mean(item_ce)
            diff = avg2 - avg1
            diff_perc = diff / avg1
            writer.writerow([k, avg1, avg2, diff, diff_perc])
        else:
            print("missing file!")
# %%
import statistics

median = statistics.median([x for (x, _) in averages])
median
# %%
fast = [(x,y) for (x,y) in averages if x <= median]
slow = [(x,y) for (x,y) in averages if x > median]
# %%
fast_increase = mean([(y - x)/ y * 100 for (x,y) in fast])
fast_increase
# %%
slow_increase = mean([(y - x)/ y * 100 for (x,y) in slow])