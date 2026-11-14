//! Our panel edge, in place of WindowGlass's thin grey gradient: a rim in
//! Windows' base colour (white in dark mode, black in light mode), strong
//! at the top and 40 % below, a full pixel all round. The theme's 31 %
//! grey at 0.3 px on the sides vanished against light wallpapers.

use crate::shell::rules::windhawk::Var;

pub const PANEL_EDGE: Var = Var::Xaml(
    "<LinearGradientBrush StartPoint=\"0,0\" EndPoint=\"0,1\"><GradientStop Color=\"{ThemeResource SystemBaseMediumColor}\" Offset=\"0\" /><GradientStop Color=\"{ThemeResource SystemBaseMediumLowColor}\" Offset=\"0.25\" /><GradientStop Color=\"{ThemeResource SystemBaseMediumLowColor}\" Offset=\"1\" /></LinearGradientBrush>",
);

pub const PANEL_EDGE_THICKNESS: Var = Var::Text("1");
