#!/bin/bash

~/bin/move-detect |
while read status;
do
	mosquitto_pub -h bb-master -t "barlog/presence" -m "${status}"
done
