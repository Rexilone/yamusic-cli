# Non-flake usage: nix-build  /  pkgs.callPackage ./path/to/yamusic-cli { }
{
  pkgs ? import <nixpkgs> { },
}:
pkgs.callPackage ./nix/package.nix { }
