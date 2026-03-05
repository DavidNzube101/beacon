// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/extensions/ERC721Enumerable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

// ─── INTERFACES ──────────────────────────────────────────────────────────────

struct Asset {
    address currency;
    uint256 premium;
    uint256 fee;
}

interface IERC7527Agency {
    function getWrapOracle(uint256 id) external view returns (Asset memory);
    function wrap(address to, bytes calldata data) external payable returns (uint256);
    function getApp() external view returns (address);
}

interface IERC7527App is IERC721Enumerable {
    function setAgency(address agency) external;
    function mint(address to, bytes calldata data) external returns (uint256);
}

// ─── IMPLEMENTATION ──────────────────────────────────────────────────────────

contract BeaconApp is ERC721Enumerable, Ownable {
    address public agency;
    uint256 public immutable maxSupply;

    constructor(string memory name, string memory symbol, uint256 _maxSupply) 
        ERC721(name, symbol) 
        Ownable(msg.sender)
    {
        maxSupply = _maxSupply;
    }

    modifier onlyAgency() {
        require(msg.sender == agency, "Only agency can mint");
        _;
    }

    function setAgency(address _agency) external onlyOwner {
        require(agency == address(0), "Agency already set");
        agency = _agency;
    }

    function mint(address to, bytes calldata data) external onlyAgency returns (uint256) {
        // Data contains the desired tokenId encoded as uint256
        uint256 tokenId = abi.decode(data, (uint256));
        require(tokenId < maxSupply, "Max supply reached");
        _safeMint(to, tokenId);
        return tokenId;
    }

    function tokenURI(uint256 tokenId) public view override returns (string memory) {
        _requireOwned(tokenId);
        // This can be extended to return a dynamic URI based on the repo URL
        return super.tokenURI(tokenId);
    }
}

contract BeaconAgency is IERC7527Agency {
    BeaconApp public immutable app;
    address public immutable currency; // address(0) for native ETH
    uint256 public immutable basePremium;
    uint256 public immutable priceStep;

    event Wrap(address indexed to, uint256 indexed tokenId, uint256 premium);

    constructor(
        address _app, 
        address _currency, 
        uint256 _basePremium, 
        uint256 _priceStep
    ) {
        app = BeaconApp(_app);
        currency = _currency;
        basePremium = _basePremium;
        priceStep = _priceStep;
    }

    function getWrapOracle(uint256) public view override returns (Asset memory) {
        uint256 supply = app.totalSupply();
        // Linear bonding curve: Price = base + (supply * step)
        uint256 premium = basePremium + (supply * priceStep);
        return Asset(currency, premium, 0);
    }

    function wrap(address to, bytes calldata data) external payable override returns (uint256) {
        Asset memory asset = getWrapOracle(0);
        
        if (currency == address(0)) {
            require(msg.value >= asset.premium, "Insufficient ETH sent");
            // Excess ETH is ignored (remains in contract or could be refunded)
        } else {
            // ERC-20 implementation would go here (requires transferFrom)
            revert("ERC-20 not implemented in this version");
        }

        // We use the current totalSupply as the next tokenId
        uint256 tokenId = app.totalSupply();
        bytes memory mintData = abi.encode(tokenId);
        
        app.mint(to, mintData);
        
        emit Wrap(to, tokenId, asset.premium);
        return tokenId;
    }

    function getApp() external view override returns (address) {
        return address(app);
    }
}
