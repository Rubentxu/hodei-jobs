#!/bin/bash
set -e
set -x  # Enable debug logging

# =============================================================================
# Complex Job: Maven Build Verification (ASDF Version)
# =============================================================================

REPO_URL="https://github.com/jenkins-docs/simple-java-maven-app.git"
PROJECT_DIR="simple-java-maven-app"

echo ">>> [START] Maven Build Verification Job via ASDF"

# 1. Environment Check & Setup
export PATH=$PATH:/usr/bin:/usr/local/bin:$HOME/.local/bin
echo ">>> [INFO] Checking environment..."
env

# Git Check
if ! command -v git &> /dev/null; then
    echo ">>> [ERR] git command not found."
    exit 1
fi
echo ">>> [INFO] Git version: $(git --version)"

# ASDF Check
if ! command -v asdf &> /dev/null; then
    echo ">>> [ERR] asdf not found in PATH."
    echo "PATH is: $PATH"
    exit 1
fi
echo ">>> [INFO] asdf version: $(asdf --version)"

# Install required dependencies for ASDF Java plugin
echo ">>> [INFO] Installing ASDF dependencies..."
if command -v apt-get &> /dev/null; then
    # Debian/Ubuntu
    sudo apt-get update -qq
    sudo apt-get install -y -qq curl wget unzip tar gpg software-properties-common 2>&1 | grep -v "^debconf:" || true
elif command -v yum &> /dev/null; then
    # RHEL/CentOS
    sudo yum install -y curl wget unzip tar gnupg2 2>&1 | grep -v "^Loaded\|^Complete" || true
elif command -v apk &> /dev/null; then
    # Alpine
    sudo apk add --no-cache curl wget unzip tar gnupg 2>&1 | grep -v "^OK" || true
fi
echo ">>> [INFO] Dependencies installed"

# 2. Configure Paths (CRITICAL for ASDF <-> Java)
# Based on Dockerfile: ASDF_DATA_DIR=/tmp/hodei/.asdf
export ASDF_DATA_DIR="${ASDF_DATA_DIR:-$HOME/.asdf}"
export PATH="$ASDF_DATA_DIR/shims:$PATH"
echo ">>> [INFO] Updated PATH: $PATH"

# Plugins
echo ">>> [INFO] Configuring plugins..."
asdf plugin list | grep -q "java" || asdf plugin add java
asdf plugin list | grep -q "maven" || asdf plugin add maven

# Versions
JAVA_VERSION="temurin-21.0.1+13"
MAVEN_VERSION="3.9.9"

# Java Install
echo ">>> [INFO] Checking Java $JAVA_VERSION..."
if ! asdf list java | grep -q "$JAVA_VERSION"; then
    echo ">>> [INFO] Installing Java (this may take time)..."
    asdf install java $JAVA_VERSION
fi
asdf global java $JAVA_VERSION

# Set JAVA_HOME
# asdf where java might fail if not fully re-shimmed, but let's try
export JAVA_HOME=$(asdf where java $JAVA_VERSION)
echo ">>> [INFO] Java Home: $JAVA_HOME"

# Maven Install
echo ">>> [INFO] Checking Maven $MAVEN_VERSION..."
if ! asdf list maven | grep -q "$MAVEN_VERSION"; then
    echo ">>> [INFO] Installing Maven..."
    asdf install maven $MAVEN_VERSION
fi
asdf global maven $MAVEN_VERSION

# 3. Reshim to ensure executables are found
asdf reshim

echo ">>> [INFO] versions set:"
java -version
mvn -version

# 2. Clone
echo ">>> [INFO] Cloning repo..."
if [ -d "$PROJECT_DIR" ]; then rm -rf "$PROJECT_DIR"; fi
git clone "$REPO_URL" "$PROJECT_DIR"
cd "$PROJECT_DIR"

# 3. Build
echo ">>> [INFO] Running Maven Build..."
mvn clean install -B

echo ">>> [SUCCESS] Build Finished"
ls -l target/
