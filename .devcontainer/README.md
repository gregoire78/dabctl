# Dev Container Setup

This directory contains configuration for the Rust development container.

## Git and SSH Configuration

VS Code can automatically forward the SSH agent from your host machine into the dev container.
It can also copy your host user's `~/.gitconfig` into the container.

Before using this setup, make sure SSH and Git are correctly configured on your host machine.

### Usage Methods

#### Bitwarden SSH Agent

1. In the Bitwarden desktop app, go to Settings and enable SSH agent.
2. Optionally configure Ask for authorization when using SSH agent.

##### Linux

Set Bitwarden as your SSH agent socket:

```bash
export SSH_AUTH_SOCK="$HOME/.bitwarden-ssh-agent.sock"
```

If Bitwarden is installed via Flatpak, use the Flatpak socket path instead:

```bash
export SSH_AUTH_SOCK="$HOME/.var/app/com.bitwarden.desktop/data/.bitwarden-ssh-agent.sock"
```

Verify which socket exists on your system:

```bash
ls -l "$HOME/.bitwarden-ssh-agent.sock" "$HOME/.var/app/com.bitwarden.desktop/data/.bitwarden-ssh-agent.sock" 2>/dev/null
```

Make it persistent in your shell profile:

```bash
echo 'export SSH_AUTH_SOCK="$HOME/.bitwarden-ssh-agent.sock"' >> ~/.bashrc
```

If you use zsh:

```bash
echo 'export SSH_AUTH_SOCK="$HOME/.bitwarden-ssh-agent.sock"' >> ~/.zshrc
```

Reload your shell and verify loaded keys:

```bash
source ~/.bashrc
ssh-add -L
```

##### Windows (WSL)

Use the same agent socket path from inside your WSL shell:

```bash
export SSH_AUTH_SOCK="$HOME/.bitwarden-ssh-agent.sock"
```

Persist it in your WSL profile:

```bash
echo 'export SSH_AUTH_SOCK="$HOME/.bitwarden-ssh-agent.sock"' >> ~/.bashrc
source ~/.bashrc
```

Then test the agent:

```bash
ssh-add -L
```

##### GUI apps (Linux, macOS, Windows)

If SSH works in terminal but fails in GUI apps, configure the agent in SSH config with `IdentityAgent`.
Many GUI apps do not inherit `SSH_AUTH_SOCK` from your shell session.

For Linux and macOS (.dmg/Homebrew Bitwarden), add this to `~/.ssh/config`:

```sshconfig
Host *
  IdentityAgent ~/.bitwarden-ssh-agent.sock
```

For macOS App Store Bitwarden, use:

```sshconfig
Host *
  IdentityAgent ~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock
```

For Windows native GUI apps (for example SourceTree, GitHub Desktop, Fork):

1. Use Bitwarden Desktop on Windows and enable SSH Agent there.
2. Configure the app to use System Git / System OpenSSH (not embedded SSH).
3. Fully quit and reopen the app after changes.

WSL note: a native Windows GUI app cannot use the Linux socket from WSL (`~/.bitwarden-ssh-agent.sock`).
If your Git operation runs on Windows, configure the Windows Bitwarden agent. If it runs in WSL/container, configure the Linux socket there.

Reference:
https://bitwarden.com/help/ssh-agent/

Additional reference (GUI apps on macOS):
https://mehmetbaykar.com/posts/bitwarden-ssh-agent-gui-apps-sourcetree-xcode/

#### Default SSH Agent

## Allowed Signers

Create the allowed signers file from your loaded SSH keys:

```bash
ssh-add -L | grep 'github-pub-key-name' | sed "s#^#$(git config user.email) #" > ~/.ssh/allowed_signers
```

Tell Git to use this file for SSH commit signature verification:

```bash
git config --global gpg.ssh.allowedSignersFile "$HOME/.ssh/allowed_signers"
```

## Test Commands

Use these commands to validate your setup end-to-end.

### 1) Check agent socket and loaded keys

```bash
echo "$SSH_AUTH_SOCK"
ls -l "$SSH_AUTH_SOCK"
ssh-add -L
```

Optional: ensure the expected key is present:

```bash
ssh-add -L | grep 'github-pub-key-name'
```

### 2) Test SSH authentication with GitHub

```bash
ssh -T git@github.com
```

Expected output is similar to:

```text
Hi <username>! You've successfully authenticated, but GitHub does not provide shell access.
```

### 3) Verify Git SSH signing configuration

```bash
git config --global --get gpg.format
git config --global --get user.signingkey
git config --global --get gpg.ssh.allowedSignersFile
```

If needed:

```bash
git config --global gpg.format ssh
git config --global user.signingkey "$(ssh-add -L | grep 'github-pub-key-name' | awk '{print $2}' | head -n1)"
```

### 4) Create and verify a signed test commit

```bash
git commit --allow-empty -m "test: ssh signing" -S
git log -1 --show-signature
```

### 5) Troubleshooting (verbose SSH)

```bash
ssh -vvv -T git@github.com
```

If GUI apps fail while terminal works, confirm they use system Git/OpenSSH and restart them fully.


