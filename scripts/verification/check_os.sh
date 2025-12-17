#!/bin/bash
sleep 5
cat /etc/os-release
whoami
id
echo "Check if apt, apk, or yum exists"
which apt-get || echo "no apt"
which apk || echo "no apk"
which yum || echo "no yum"
